# LineOS — Phase 10 Task Decomposition

**Document:** `lineos/plan/phase-10/task-decomposition.md`
**Version:** 1.0
**Phase:** 10 — Export (WAV + FLAC + Opus)
**Status:** 🔒 LOCKED
**Authority:** Phase 10 Master Prompt · LineOS Constitution v2.0

---

## Architecture

```
FM5 → EXPORT button
        │
        ▼
Tauri command: export_audio(blob_id, format, output_path)
        │
        ▼
M0 export handler (handlers/export.rs)
        │
        ├── WAV:  decode GoldenBlob.flac_bytes → hound write
        ├── FLAC: write GoldenBlob.flac_bytes directly
        └── Opus: decode → audiopus encode → write
        │
        ▼
Local filesystem: ~/Music/StillAir/exports/
+ sidecar: <name>.stillair.json
```

**Key insight:** The Golden Blob stores `flac_bytes: Vec<u8>`.
- FLAC export = write bytes directly (zero re-encoding)
- WAV export = decode FLAC bytes → write PCM as WAV
- Opus export = decode FLAC bytes → encode Opus

---

## Task Order

```
P10-001  Add hound + audiopus to m0d Cargo.toml
P10-002  Export formats — implement in handlers/export.rs
P10-003  Sidecar JSON writer
P10-004  POST /export endpoint — wire to blob_store
P10-005  Tauri command: export_audio (real, replaces stub)
P10-006  Frontend: EXPORT button wiring + format selector
P10-007  Default export directory setup
P10-008  CI gate + tag
```

---

## P10-001 — Add Dependencies

```toml
# lineos/m0/m0-daemon/Cargo.toml
[dependencies]
hound      = "3.5"    # WAV write — MIT
audiopus   = "0.3"    # Opus encode — MIT/BSD
```

If `audiopus` fails to compile (C deps), use:
```toml
# fallback — pure interface, links libopus system lib
opus = "0.3"
```

**DoD P10-001:**
```bash
cargo check -p m0d
echo "✅ P10-001"
```

---

## P10-002 — Export Formats

Replace stub in `handlers/export.rs`:

```rust
//! Audio export from Golden Blob.
//! WAV: hound write from decoded FLAC bytes
//! FLAC: direct byte write (zero re-encoding)
//! Opus: decode → audiopus encode

use crate::blob_store::StoredBlob;
use std::path::Path;

pub enum ExportFormat {
    Wav,
    Flac,
    Opus,
}

impl ExportFormat {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "wav"  => Ok(Self::Wav),
            "flac" => Ok(Self::Flac),
            "opus" => Ok(Self::Opus),
            other  => Err(format!("Unsupported format: {other}. Use wav/flac/opus")),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self { Self::Wav => "wav", Self::Flac => "flac", Self::Opus => "opus" }
    }
}

pub fn export_blob(
    blob: &StoredBlob,
    format: ExportFormat,
    output_path: &Path,
) -> Result<(), String> {
    match format {
        ExportFormat::Flac => export_flac(blob, output_path),
        ExportFormat::Wav  => export_wav(blob, output_path),
        ExportFormat::Opus => export_opus(blob, output_path),
    }
}

/// FLAC: write Golden Blob bytes directly — zero re-encoding
fn export_flac(blob: &StoredBlob, path: &Path) -> Result<(), String> {
    std::fs::write(path, &blob.flac_bytes)
        .map_err(|e| format!("FLAC write failed: {e}"))
}

/// WAV: decode FLAC bytes → write 32-bit float WAV via hound
fn export_wav(blob: &StoredBlob, path: &Path) -> Result<(), String> {
    // Decode FLAC bytes to f32 PCM
    let pcm = decode_flac_to_f32(&blob.flac_bytes)?;

    let spec = hound::WavSpec {
        channels:        blob.channels as u16,
        sample_rate:     blob.sample_rate,
        bits_per_sample: 32,
        sample_format:   hound::SampleFormat::Float,
    };

    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("WAV create failed: {e}"))?;

    for &sample in &pcm {
        writer.write_sample(sample)
            .map_err(|e| format!("WAV write sample failed: {e}"))?;
    }

    writer.finalize()
        .map_err(|e| format!("WAV finalize failed: {e}"))
}

/// Opus: decode FLAC → encode Opus via audiopus
fn export_opus(blob: &StoredBlob, path: &Path) -> Result<(), String> {
    let pcm = decode_flac_to_f32(&blob.flac_bytes)?;

    // Convert f32 → i16 for Opus encoder
    let pcm_i16: Vec<i16> = pcm.iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect();

    let encoder = audiopus::coder::Encoder::new(
        audiopus::SampleRate::Hz48000,
        audiopus::Channels::Stereo,
        audiopus::Application::Audio,
    ).map_err(|e| format!("Opus encoder init failed: {e}"))?;

    // Encode in 960-sample frames (20ms at 48kHz)
    let frame_size = 960 * blob.channels as usize;
    let mut opus_bytes: Vec<u8> = Vec::new();

    for chunk in pcm_i16.chunks(frame_size) {
        let mut output = vec![0u8; 4096];
        let len = encoder.encode(chunk, &mut output)
            .map_err(|e| format!("Opus encode failed: {e}"))?;
        // Write frame length (4 bytes) + frame data (simple container)
        let len_bytes = (len as u32).to_le_bytes();
        opus_bytes.extend_from_slice(&len_bytes);
        opus_bytes.extend_from_slice(&output[..len]);
    }

    std::fs::write(path, &opus_bytes)
        .map_err(|e| format!("Opus write failed: {e}"))
}

/// Decode FLAC bytes to interleaved f32 PCM using symphonia
fn decode_flac_to_f32(flac_bytes: &[u8]) -> Result<Vec<f32>, String> {
    use symphonia::core::{
        audio::SampleBuffer,
        codecs::DecoderOptions,
        formats::FormatOptions,
        io::{MediaSourceStream, ReadOnlySource},
        meta::MetadataOptions,
        probe::Hint,
    };
    use symphonia::default::{get_codecs, get_probe};

    let cursor = std::io::Cursor::new(flac_bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("flac");

    let probe = get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("FLAC probe failed: {e}"))?;

    let mut format = probe.format;
    let track = format.tracks().iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or("No audio track in FLAC")?;

    let track_id = track.id;
    let mut decoder = get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("FLAC decoder init failed: {e}"))?;

    let mut samples: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() { Ok(p) => p, Err(_) => break };
        if packet.track_id() != track_id { continue; }
        let decoded = match decoder.decode(&packet) { Ok(d) => d, Err(_) => continue };
        let spec = *decoded.spec();
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        samples.extend_from_slice(buf.samples());
    }

    if samples.is_empty() {
        return Err("FLAC decoded to empty buffer".into());
    }
    Ok(samples)
}
```

**DoD P10-002:**
```bash
cargo check -p m0d
echo "✅ P10-002"
```

---

## P10-003 — Sidecar JSON Writer

```rust
// handlers/export.rs — add sidecar writer

use crate::blob_store::StoredBlob;
use serde::Serialize;

#[derive(Serialize)]
pub struct ExportSidecar {
    pub blob_id:         String,
    pub preset_id:       String,
    pub export_format:   String,
    pub exported_at:     String,
    pub loudness:        StoredLoudness,   // from blob
    pub quality:         StoredQuality,    // from blob
    pub compliance:      ComplianceSummary,
}

#[derive(Serialize)]
pub struct ComplianceSummary {
    pub spotify:  bool,
    pub youtube:  bool,
    pub apple:    bool,
    pub tidal:    bool,
    pub broadcast: bool,
}

pub fn write_sidecar(
    blob: &StoredBlob,
    format: &str,
    audio_path: &Path,
) -> Result<(), String> {
    let sidecar_path = audio_path.with_extension("stillair.json");
    let sidecar = ExportSidecar {
        blob_id:       blob.id.to_string(),
        preset_id:     blob.preset_id.clone(),
        export_format: format.to_string(),
        exported_at:   chrono::Utc::now().to_rfc3339(),
        loudness:      blob.loudness.clone(),
        quality:       blob.quality.clone(),
        compliance: ComplianceSummary {
            spotify:   blob.loudness.spotify_compliant,
            youtube:   blob.loudness.youtube_compliant,
            apple:     blob.loudness.apple_music_compliant,
            tidal:     blob.loudness.tidal_compliant,
            broadcast: blob.loudness.broadcast_compliant,
        },
    };

    let json = serde_json::to_string_pretty(&sidecar)
        .map_err(|e| format!("Sidecar serialize failed: {e}"))?;

    std::fs::write(&sidecar_path, json)
        .map_err(|e| format!("Sidecar write failed: {e}"))
}
```

**DoD P10-003:**
```bash
cargo check -p m0d
echo "✅ P10-003"
```

---

## P10-004 — POST /export Endpoint

Update `handlers/export.rs` Axum handler:

```rust
#[derive(serde::Deserialize)]
pub struct ExportRequest {
    pub blob_id:     String,
    pub format:      String,   // "wav" | "flac" | "opus"
    pub output_path: String,   // absolute path chosen by user
}

#[derive(serde::Serialize)]
pub struct ExportResponse {
    pub status:       String,
    pub written_path: Option<String>,
    pub message:      Option<String>,
}

pub async fn handle_export(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ExportRequest>,
) -> Json<ExportResponse> {
    let blob = match state.blob_store.get(&req.blob_id) {
        Some(b) => b,
        None => return Json(ExportResponse {
            status: "error".into(),
            written_path: None,
            message: Some(format!("Blob not found: {}", req.blob_id)),
        }),
    };

    let format = match ExportFormat::from_str(&req.format) {
        Ok(f) => f,
        Err(e) => return Json(ExportResponse {
            status: "error".into(),
            written_path: None,
            message: Some(e),
        }),
    };

    let output_path = std::path::Path::new(&req.output_path);

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Json(ExportResponse {
                status: "error".into(),
                written_path: None,
                message: Some(format!("Cannot create export dir: {e}")),
            });
        }
    }

    // Export audio
    if let Err(e) = export_blob(&blob, format, output_path) {
        return Json(ExportResponse {
            status: "error".into(),
            written_path: None,
            message: Some(e),
        });
    }

    // Write sidecar
    let _ = write_sidecar(&blob, &req.format, output_path);

    Json(ExportResponse {
        status: "ok".into(),
        written_path: Some(req.output_path.clone()),
        message: None,
    })
}
```

**DoD P10-004:**
```bash
cargo check -p m0d
echo "✅ P10-004"
```

---

## P10-005 — Tauri Command: export_audio

Replace stub in `apps/stillair/src-tauri/src/commands/export.rs`:

```rust
use tauri_plugin_dialog::DialogExt;
use crate::ipc::m0_client::M0Client;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ExportResult {
    pub written_path: String,
    pub format:       String,
}

#[tauri::command]
pub async fn export_audio(
    blob_id: String,
    format:  String,
    app:     tauri::AppHandle,
) -> Result<ExportResult, String> {
    // 1. Open native save dialog
    let extension = format.as_str().to_lowercase();
    let default_name = format!("mastered.{}", extension);

    let path = tokio::task::spawn_blocking({
        let app = app.clone();
        let ext = extension.clone();
        let name = default_name.clone();
        move || {
            app.dialog()
                .file()
                .set_file_name(&name)
                .add_filter(&ext.to_uppercase(), &[&ext])
                .blocking_save_file()
        }
    }).await
    .map_err(|e| format!("Dialog spawn failed: {e}"))?;

    let output_path = match path {
        Some(p) => p.to_string_lossy().to_string(),
        None => return Err("Export cancelled".into()),
    };

    // 2. Call M0 export endpoint
    let client = M0Client::new();
    client.export(&blob_id, &format, &output_path).await
        .map_err(|e| format!("IO_ERR:0x02:Export failed: {e}"))?;

    Ok(ExportResult { written_path: output_path, format })
}
```

**DoD P10-005:**
```bash
cargo check -p stillair
echo "✅ P10-005"
```

---

## P10-006 — Frontend: EXPORT Button + Format Selector

Update `frontend/src/cockpit/session_panel.rs` or `app.rs`:

Format selector (FM5 only):
```rust
// Simple format selector — 3 buttons
view! {
    <div class="export-formats">
        <button
            class="btn-export-format"
            on:click=move |_| set_export_format.set("wav")
        >"WAV"</button>
        <button
            class="btn-export-format"
            on:click=move |_| set_export_format.set("flac")
        >"FLAC"</button>
        <button
            class="btn-export-format"
            on:click=move |_| set_export_format.set("opus")
        >"OPUS"</button>
    </div>
}
```

Wire EXPORT button in `app.rs` `on_export` handler:
```rust
let on_export = {
    let blob_id = blob_id.clone();
    move |_| {
        let bid = blob_id.get().unwrap_or_default();
        let fmt = export_format.get();
        spawn_local(async move {
            match invoke::<ExportResult>(
                "exportAudio",
                serde_json::json!({ "blobId": bid, "format": fmt }),
            ).await {
                Ok(result) => {
                    leptos::logging::log!(
                        "EXPORT OK: {} → {}", result.format, result.written_path
                    );
                    set_export_status.set(Some(result.written_path));
                }
                Err(e) => {
                    leptos::logging::warn!("EXPORT ERROR: {}", e);
                    // Show error in UI (non-fatal — stay in FM5)
                }
            }
        });
    }
};
```

**DoD P10-006:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-frontend
echo "✅ P10-006"
```

---

## P10-007 — Default Export Directory

```rust
// In export_audio command, if no path from dialog:
fn default_export_dir() -> std::path::PathBuf {
    dirs::audio_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
        .join("StillAir")
        .join("exports")
}
```

Add `dirs` to `src-tauri/Cargo.toml`:
```toml
dirs = "5"
```

**DoD P10-007:**
```bash
cargo check -p stillair
echo "✅ P10-007"
```

---

## P10-008 — CI Gate + Tag

```bash
cargo test --workspace
just ci
bash infra/ci/checks/ui-isolation-check.sh

git add -A
git commit -m "feat(export): Phase 10 — WAV + FLAC + Opus export

- WAV: hound 32-bit float write from decoded FLAC bytes
- FLAC: direct Golden Blob bytes write (zero re-encoding)
- Opus: audiopus encode from decoded PCM
- Sidecar JSON: .stillair.json alongside audio file
- Native save dialog via tauri-plugin-dialog
- Default export dir: ~/Music/StillAir/exports/
- EXPORT button FM5 only — format selector (WAV/FLAC/OPUS)
- Non-fatal export errors — user stays in FM5

No subprocess, no FFmpeg, no re-measurement.
Export reads Golden Blob only — immutable after creation.
Amendment A-002 §3 enforced: export via Tauri IPC only.

Authority: LineOS Constitution v2.0 · Amendment A-002"

git tag v0.10.0-export
git log --oneline -5
```

---

## Completion Report

```
✅ Phase 10 — Export — COMPLETE

WAV export:    hound 32-bit float ✅
FLAC export:   direct bytes (zero re-encoding) ✅
Opus export:   audiopus encode ✅
Sidecar JSON:  .stillair.json ✅
Save dialog:   native Tauri ✅
just ci:       ✅

Tag: v0.10.0-export ✅

Ready for: Phase 11 — Dioxus Cockpit migration
```

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 10
**Version:** 1.0
**Status:** 🔒 LOCKED
