# LineOS — Phase 7 Task Decomposition

**Document:** `lineos/plan/phase-7/task-decomposition.md`
**Version:** 1.0
**Phase:** 7 — Audio Decode (symphonia)
**Status:** 🔒 LOCKED
**Authority:** Phase 7 Master Prompt · LineOS Constitution v2.0

---

## Architectural Decision (Binding)

```
M0 master handler
    │
    ├── decode_audio()     ← NEW — symphonia (WAV/FLAC/MP3/AIFF → f32 PCM)
    │       │
    │       └── AudioPcm { samples: Vec<f32>, sample_rate: u32, channels: u16 }
    │
    ▼
sp314-dsp MasteringPipeline::master(AudioPcm, MasteringIntent)
    │
    ▼
GoldenBlob (real loudness metrics)
```

symphonia lives in `lineos/m0/m0-daemon/Cargo.toml`.
sp314-dsp never imports symphonia. Ever.

---

## Task Order

```
P7-001  Add symphonia to m0d Cargo.toml
P7-002  decode_audio() function (WAV/FLAC/MP3/AIFF)
P7-003  Replace bytes_to_f32_samples() stub in master.rs
P7-004  Update AudioPcm type + MasteringIntent wiring
P7-005  Decode tests
P7-006  CI gate + tag
```

---

## P7-001 — Add symphonia + rubato to m0d Cargo.toml

```toml
# lineos/m0/m0-daemon/Cargo.toml
[dependencies]
# ... existing deps ...
symphonia = { version = "0.5", features = [
    "mp3",
    "flac",
    "pcm",
    "aiff",
    "wav",
    "isomp4",
] }
rubato = "0.15"   # resampling to 48000 Hz
```

symphonia license: MIT. rubato license: MIT. Both in deny.toml allow list.

**DoD P7-001:**
```bash
cargo check -p m0d
echo "✅ P7-001"
```

---

## P7-002 — decode_audio() Function

Create `lineos/m0/m0-daemon/src/handlers/decode.rs`.

Full implementation with:
- File size check (max 500MB) before decode
- symphonia decode (WAV/FLAC/MP3/AIFF → f32 PCM)
- Duration check (max 720s / 12 minutes)
- Channel normalization → stereo (mono→stereo, N-ch downmix)
- Resample to 48000 Hz if needed (rubato SincFixedIn)
- Always returns sample_rate=48000, channels=2

Key types:
```rust
pub const TARGET_SAMPLE_RATE: u32 = 48000;
pub const TARGET_CHANNELS: u16 = 2;
pub const MAX_DURATION_SECS: u64 = 720;  // 12 minutes
pub const MAX_FILE_BYTES: u64 = 500 * 1024 * 1024;

pub struct AudioPcm {
    pub samples:     Vec<f32>,  // interleaved, 48000 Hz, stereo
    pub sample_rate: u32,       // always 48000
    pub channels:    u16,       // always 2
    pub duration_ms: u64,
    pub original_sr: u32,       // original sample rate (for audit log)
    pub original_ch: u16,       // original channels (for audit log)
}

pub enum DecodeError {
    FileNotFound(String),
    FileTooLarge(u64),
    DurationExceeded(u64),
    UnsupportedFormat(String),
    DecodeFailure(String),
    ResampleFailure(String),
}
```

Decode pipeline:
```
1. fs::metadata → size check → DecodeError::FileTooLarge if > 500MB
2. symphonia probe + decode → raw Vec<f32> at original_sr/original_ch
3. frame count / original_sr → duration check → DecodeError::DurationExceeded
4. channel normalize:
   - mono (1ch)  → mono_to_stereo() — duplicate each sample
   - stereo (2ch)→ pass through
   - N ch        → downmix_to_stereo() — average pairs, clamp [-1,1]
5. resample if original_sr != 48000:
   - rubato SincFixedIn, ratio = 48000.0 / original_sr
   - de-interleave → resample per channel → re-interleave
6. return AudioPcm { samples, sample_rate: 48000, channels: 2, ... }
```

After decode, write audit log entry:
```rust
audit.write_event(AuditEntry::new("m0d.audio_decoded")
    .with_field("path", path)
    .with_field("original_sr", original_sr)
    .with_field("original_ch", original_ch)
    .with_field("duration_ms", duration_ms)
    .with_field("resampled", original_sr != TARGET_SAMPLE_RATE));
```

**DoD P7-002:**
```bash
cargo check -p m0d
echo "✅ P7-002"
```

---

## P7-003 — Replace Stub in master.rs

**Goal:** Replace `bytes_to_f32_samples()` and `read_audio_file()` stubs
with `decode_audio()`.

In `handlers/master.rs`:

```rust
// Add at top
mod decode;  // or pub(super) mod decode — already in handlers/

// Replace the stub run_dsp function:
async fn run_dsp(
    path: &str,
    preset_id: &str,
    schema: &Bmr128Schema,
) -> Result<GoldenBlob, String> {

    // 1. Real decode — replaces bytes_to_f32_samples() stub
    let pcm = decode::decode_audio(path)
        .map_err(|e| format!("Decode error: {}", e))?;

    // 2. Validate — not silence
    let rms: f32 = {
        let sum: f32 = pcm.samples.iter().map(|&s| s * s).sum();
        libm::sqrtf(sum / pcm.samples.len() as f32)
    };
    let rms_db = 20.0 * libm::log10f(rms.max(1e-10));
    if rms_db < -60.0 {
        return Err(format!(
            "Input validation failed: audio is silence (RMS = {:.1} dBFS)",
            rms_db
        ));
    }

    // 3. Build AudioChunk from decoded PCM
    let chunk = AudioChunk {
        samples:     pcm.samples,
        sample_rate: pcm.sample_rate,
        channels:    pcm.channels,
    };

    // 4. Build MasteringIntent from schema
    let preset = schema.presets.get(preset_id);
    let target_lufs = preset
        .and_then(|p| p.target_lufs)
        .unwrap_or(-14.0)
        .clamp(-40.0, 0.0);

    let input_hash = sha2_hash(path.as_bytes());
    let seed = u64::from_le_bytes(input_hash[..8].try_into().unwrap_or([0u8; 8]));

    let intent = MasteringIntent {
        seed,
        target_lufs,
        export_16bit: false,
    };

    // 5. Run sp314-dsp pipeline
    let pipeline = MasteringPipeline::new(schema.pipeline.clone());
    let blob = pipeline
        .master(chunk, intent)
        .map_err(|e| format!("DSP pipeline error: {:?}", e))?;

    Ok(blob)
}
```

**DoD P7-003:**
```bash
cargo check -p m0d
echo "✅ P7-003"
```

---

## P7-004 — Update handlers/mod.rs

Add `decode` module to handlers:

```rust
// lineos/m0/m0-daemon/src/handlers/mod.rs
pub mod blob;
pub mod decode;    // ← new
pub mod export;
pub mod master;
```

**DoD P7-004:**
```bash
cargo check -p m0d
echo "✅ P7-004"
```

---

## P7-005 — Decode Tests

Add to `handlers/decode.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_nonexistent_file() {
        let result = decode_audio("/nonexistent/file.wav");
        assert!(matches!(result, Err(DecodeError::FileNotFound(_))));
    }

    #[test]
    fn test_decode_invalid_format() {
        let path = "/tmp/test_decode_not_audio.txt";
        std::fs::write(path, b"not audio data").unwrap();
        let result = decode_audio(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_target_constants() {
        assert_eq!(TARGET_SAMPLE_RATE, 48000);
        assert_eq!(TARGET_CHANNELS, 2);
        assert_eq!(MAX_DURATION_SECS, 720);
        assert_eq!(MAX_FILE_BYTES, 500 * 1024 * 1024);
    }

    #[test]
    fn test_mono_to_stereo() {
        let mono = vec![0.5f32, -0.5, 0.25];
        let stereo = mono_to_stereo(&mono);
        assert_eq!(stereo, vec![0.5, 0.5, -0.5, -0.5, 0.25, 0.25]);
    }

    #[test]
    fn test_decode_error_display() {
        assert!(DecodeError::FileTooLarge(600_000_000).to_string().contains("500MB"));
        assert!(DecodeError::DurationExceeded(800).to_string().contains("12 min"));
    }
}
```

Integration test (manual — requires real audio file):
```bash
# After rebuilding m0d:
curl -X POST http://127.0.0.1:7400/master \
  -H "Content-Type: application/json" \
  -d '{"audio_path": "/home/aidevcon/Downloads/gargar.mp3", "preset_id": "spotify"}'

# Expected:
# - blob_id: non-empty UUID
# - integrated_lufs: real value (not -2.84 stub)
# - lra: > 0.0
# - M0 terminal shows: "m0d.audio_decoded" audit entry
```

**DoD P7-005:**
```bash
cargo test -p m0d
echo "✅ P7-005"
```

---

## P7-006 — CI Gate + Tag

```bash
cargo test -p m0d
cargo test --workspace
just ci
just deny

git add -A
git commit -m "feat(decode): Phase 7 — symphonia audio decode in M0

- decode_audio() in handlers/decode.rs (symphonia)
- Supports: WAV, FLAC, MP3, AIFF
- Returns AudioPcm { samples: Vec<f32>, sample_rate, channels, duration_ms }
- Replaces bytes_to_f32_samples() stub in handlers/master.rs
- sp314-dsp receives real f32 PCM — never compressed audio
- Golden Blob loudness metrics are now accurate
- Architecture: M0 decodes, sp314-dsp processes (constitution enforced)

Authority: LineOS Constitution v2.0 · M0 Constitution v2.0"

git tag v0.7.0-decode
git log --oneline -5
```

---

## Completion Report

```
✅ Phase 7 — Audio Decode — COMPLETE

symphonia:          WAV/FLAC/MP3/AIFF → f32 PCM ✅
decode location:    M0 handlers/decode.rs ✅
sp314-dsp:          f32 PCM only (never compressed) ✅
Golden Blob:        real loudness metrics ✅
just ci:            ✅

Tag: v0.7.0-decode ✅

Ready for: Phase 8 — Aether coaching (LLM narrative)
```

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 7
**Version:** 1.0
**Status:** 🔒 LOCKED
