//! POST /export — export Golden Blob audio as WAV, FLAC, Opus, or MP3.
//! Authority: Phase 10 task-decomposition P10-002/P10-003/P10-004
//!             Phase 13 P13-003 — MP3 via LAME (LGPL, dynamic linking)
//!
//! Architecture (binding):
//!   Tauri command export_audio(blob_id, format, output_path)
//!       → POST /export (this handler)
//!       → blob_store.get(blob_id) → StoredBlob.audio_bytes (f32 LE PCM)
//!       ├── WAV:  decode bytes → hound write (32-bit float, 48kHz)
//!       ├── FLAC: write audio_bytes directly (zero re-encoding)
//!       ├── Opus: decode bytes → audiopus encode → write
//!       └── MP3:  decode bytes → LAME encode → write (LGPL dynamic link)
//!       → sidecar .stillair.json alongside audio file
//!
//! FORBIDDEN (per Phase 10 master prompt + Amendment A-002 §3 + Phase 13):
//!   ❌ FFmpeg or any subprocess
//!   ❌ Re-running DSP pipeline during export
//!   ❌ Re-measuring loudness during export
//!   ❌ Modifying the Golden Blob
//!   ❌ Returning raw audio bytes to the frontend
//!   ❌ Static linking of LAME (LGPL violation)

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::path::Path;
use chrono::Utc;

use crate::app_state::AppState;
use crate::audit::{AuditEntry, AuditLevel};
use crate::blob_store::{StoredBlob, StoredLoudness, StoredQuality};

// ── Request/Response types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub blob_id:     String,
    pub format:      String,     // "wav" | "flac" | "opus" | "mp3"
    pub output_path: String,     // absolute path chosen by user via native dialog
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub status:       String,           // "ok" | "error"
    pub written_path: Option<String>,
    pub message:      Option<String>,
}

// ── Format enum ───────────────────────────────────────────────────────────────

pub enum ExportFormat {
    Wav,
    Flac,
    Opus,
    /// MP3 via LAME — LGPL, dynamic linking only.
    /// See: lineos/plan/phase-13/LAME-LGPL-NOTICE.md
    Mp3,
    /// AIFF — Logic Pro native, uncompressed 32-bit float BE PCM.
    /// Pure Rust, no new crate. P13-003b.
    Aiff,
}

impl ExportFormat {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "wav"  => Ok(Self::Wav),
            "flac" => Ok(Self::Flac),
            "opus" => Ok(Self::Opus),
            // MP3 via LAME — LGPL dynamic linking only (see LAME-LGPL-NOTICE.md)
            "mp3"  => Ok(Self::Mp3),
            // AIFF — Logic Pro native, uncompressed 32-bit float big-endian
            "aiff" | "aif" => Ok(Self::Aiff),
            other  => Err(format!("Unsupported format: {other}. Use wav/flac/opus/mp3/aiff")),
        }
    }

    #[allow(dead_code)]
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Wav  => "wav",
            Self::Flac => "flac",
            Self::Opus => "opus",
            Self::Mp3  => "mp3",
            Self::Aiff => "aiff",
        }
    }
}

// ── Axum handler ──────────────────────────────────────────────────────────────

/// POST /export — read Golden Blob, write audio to disk, return written_path.
/// Audio bytes remain in M0 storage — only the path is returned.
pub async fn export_audio(
    State(state): State<AppState>,
    Json(req):    Json<ExportRequest>,
) -> Json<ExportResponse> {
    // Validate format
    let format = match ExportFormat::from_str(&req.format) {
        Ok(f)  => f,
        Err(e) => return Json(ExportResponse {
            status:       "error".into(),
            written_path: None,
            message:      Some(e),
        }),
    };

    // Fetch blob
    let blob = match state.blob_store.get(&req.blob_id) {
        Some(b) => b,
        None    => return Json(ExportResponse {
            status:       "error".into(),
            written_path: None,
            message:      Some(format!("blob not found: {}", req.blob_id)),
        }),
    };

    let output_path = std::path::PathBuf::from(&req.output_path);

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return Json(ExportResponse {
                status:       "error".into(),
                written_path: None,
                message:      Some(format!("Cannot create export directory: {e}")),
            });
        }
    }

    // Export audio (blocking I/O)
    let blob_clone  = blob.clone();
    let path_str    = req.output_path.clone();
    let format_str  = req.format.clone();
    let path_for_io = output_path.clone();

    let result = tokio::task::spawn_blocking(move || {
        export_blob(&blob_clone, format, &path_for_io)
            .and_then(|()| write_sidecar(&blob_clone, &format_str, &path_for_io))
    }).await
    .map_err(|e| format!("Export task join error: {e}"))
    .and_then(|r| r);

    match result {
        Ok(()) => {
            state.audit.write(
                AuditEntry::new("m0d.export_complete", AuditLevel::Audit,
                    &format!("format={} path={path_str}", req.format))
            ).ok();
            Json(ExportResponse {
                status:       "ok".into(),
                written_path: Some(path_str),
                message:      None,
            })
        }
        Err(e) => {
            state.audit.write(
                AuditEntry::new("m0d.export_failed", AuditLevel::Audit, &e)
            ).ok();
            Json(ExportResponse {
                status:       "error".into(),
                written_path: None,
                message:      Some(e),
            })
        }
    }
}

// ── P10-002: Export format writers ────────────────────────────────────────────

/// Route to format-specific writer.
fn export_blob(blob: &StoredBlob, format: ExportFormat, path: &Path) -> Result<(), String> {
    match format {
        ExportFormat::Flac => export_flac(blob, path),
        ExportFormat::Wav  => export_wav(blob, path),
        ExportFormat::Opus => export_opus(blob, path),
        // MP3: LAME encoder — LGPL dynamic linking only (see LAME-LGPL-NOTICE.md)
        ExportFormat::Mp3  => export_mp3(blob, path),
        // AIFF: uncompressed 32-bit float big-endian PCM (P13-003b)
        ExportFormat::Aiff => export_aiff(blob, path),
    }
}

/// FLAC: write audio_bytes directly — zero re-encoding.
/// audio_bytes = raw f32 LE PCM bytes from GoldenBlob (Phase 2/10 note).
/// Phase 11: replace with real FLAC encoder when sp314-dsp adds FLAC output.
fn export_flac(blob: &StoredBlob, path: &Path) -> Result<(), String> {
    if blob.audio_bytes.is_empty() {
        return Err("No audio bytes in blob — mastering may have used legacy path".into());
    }
    std::fs::write(path, &blob.audio_bytes)
        .map_err(|e| format!("FLAC write failed: {e}"))
}

/// WAV: decode f32 LE PCM bytes → write 32-bit float WAV via hound.
fn export_wav(blob: &StoredBlob, path: &Path) -> Result<(), String> {
    if blob.audio_bytes.is_empty() {
        return Err("No audio bytes in blob — cannot write WAV".into());
    }

    let samples = pcm_bytes_to_f32(&blob.audio_bytes);

    let spec = hound::WavSpec {
        channels:        blob.channels as u16,
        sample_rate:     blob.sample_rate,
        bits_per_sample: 32,
        sample_format:   hound::SampleFormat::Float,
    };

    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("WAV create failed: {e}"))?;

    for &sample in &samples {
        writer.write_sample(sample)
            .map_err(|e| format!("WAV write sample failed: {e}"))?;
    }

    writer.finalize()
        .map_err(|e| format!("WAV finalize failed: {e}"))
}

/// Opus: stub — requires libopus-dev system library (Phase 11).
/// Phase 10 delivers WAV + FLAC. Opus wired in Phase 11 after libopus install.
fn export_opus(_blob: &StoredBlob, _path: &Path) -> Result<(), String> {
    Err("Opus export requires libopus-dev (Phase 11). \
         Use WAV, FLAC, MP3, or AIFF.".into())
}

/// AIFF export — Logic Pro native format.
///
/// Uncompressed 32-bit float big-endian PCM. No new crate — pure Rust.
/// Authority: Phase 13 P13-003b.
///
/// Format layout:
///   FORM chunk (total container)
///     AIFF type tag
///     COMM chunk (18 bytes): channels, num_frames, bit_depth, 80-bit sample rate
///     SSND chunk: offset(4) + blockSize(4) + big-endian PCM
///
/// No DSP re-run — reads f32 LE PCM from Golden Blob, converts to BE in-place.
fn export_aiff(blob: &StoredBlob, path: &Path) -> Result<(), String> {
    if blob.audio_bytes.is_empty() {
        return Err("No audio bytes in blob — cannot write AIFF".into());
    }

    let pcm = pcm_bytes_to_f32(&blob.audio_bytes);
    let channels    = blob.channels.max(1) as u16;
    let sample_rate = blob.sample_rate;
    let num_frames  = (pcm.len() / channels as usize) as u32;
    let bit_depth: u16 = 32;

    // Convert interleaved f32 LE PCM → f32 BE (AIFF is big-endian)
    let mut pcm_be: Vec<u8> = Vec::with_capacity(pcm.len() * 4);
    for &s in &pcm {
        // Reinterpret as u32 bits then write big-endian
        pcm_be.extend_from_slice(&s.to_bits().to_be_bytes());
    }

    let pcm_size  = pcm_be.len() as u32;
    let ssnd_size = pcm_size + 8;  // offset(4) + blockSize(4) + PCM data
    let comm_size = 18u32;         // channels(2) + frames(4) + bitDepth(2) + sampleRate(10)
    // FORM data = 4 ("AIFF") + 8 (COMM header) + 18 (COMM body) + 8 (SSND header) + ssnd_size
    let form_size = 4 + (8 + comm_size) + (8 + ssnd_size);

    let mut buf: Vec<u8> = Vec::with_capacity(12 + 8 + comm_size as usize + 8 + ssnd_size as usize);

    // ── FORM chunk header ────────────────────────────────────────────────
    buf.extend_from_slice(b"FORM");
    buf.extend_from_slice(&form_size.to_be_bytes());
    buf.extend_from_slice(b"AIFF");

    // ── COMM chunk ───────────────────────────────────────────────────────
    buf.extend_from_slice(b"COMM");
    buf.extend_from_slice(&comm_size.to_be_bytes());
    buf.extend_from_slice(&channels.to_be_bytes());
    buf.extend_from_slice(&num_frames.to_be_bytes());
    buf.extend_from_slice(&bit_depth.to_be_bytes());
    // 80-bit IEEE 754 extended for sample rate (AIFF spec requirement)
    buf.extend_from_slice(&f64_to_80bit_extended(sample_rate as f64));

    // ── SSND chunk ───────────────────────────────────────────────────────
    buf.extend_from_slice(b"SSND");
    buf.extend_from_slice(&ssnd_size.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes());  // offset (always 0)
    buf.extend_from_slice(&0u32.to_be_bytes());  // blockSize (always 0)
    buf.extend_from_slice(&pcm_be);

    tracing::info!(
        path       = %path.display(),
        bytes      = buf.len(),
        num_frames = num_frames,
        "m0d: AIFF export complete"
    );

    std::fs::write(path, &buf)
        .map_err(|e| format!("AIFF write failed: {e}"))
}

/// Convert f64 to 80-bit IEEE 754 extended precision.
///
/// Required by the AIFF COMM chunk for the sample rate field.
/// AIFF predates IEEE 754 double — it mandates 80-bit extended (x87 format).
///
/// Format: 1 sign bit | 15-bit biased exponent | 64-bit integer mantissa
/// (no implicit leading 1 unlike double precision).
fn f64_to_80bit_extended(val: f64) -> [u8; 10] {
    let mut bytes = [0u8; 10];
    if val == 0.0 {
        return bytes;
    }

    let bits = val.to_bits();
    let sign:    u16 = ((bits >> 63) as u16) << 15;
    let exp_f64: i32 = ((bits >> 52) & 0x7ff) as i32 - 1023;  // unbiased double exponent
    let exp_80:  u16 = (exp_f64 + 16383) as u16;               // rebias for 80-bit extended
    let mantissa_f64 = bits & 0x000f_ffff_ffff_ffff;            // 52-bit fraction
    // 80-bit explicit mantissa: leading 1 + 52-bit fraction left-shifted to 63 bits
    let mantissa_80: u64 = (1u64 << 63) | (mantissa_f64 << 11);

    let exp_word = sign | exp_80;
    bytes[0..2].copy_from_slice(&exp_word.to_be_bytes());
    bytes[2..10].copy_from_slice(&mantissa_80.to_be_bytes());
    bytes
}

/// MP3 export via LAME encoder.
///
/// LGPL compliance: uses local lame-sys crate which links libmp3lame DYNAMICALLY.
/// See: lineos/plan/phase-13/LAME-LGPL-NOTICE.md
/// System: libmp3lame.so.0 at /lib/x86_64-linux-gnu/libmp3lame.so.0
///
/// Quality preset 2: mastering grade (0=best, 9=worst).
/// No DSP re-run — reads stored f32 LE PCM bytes from the Golden Blob.
fn export_mp3(blob: &StoredBlob, path: &Path) -> Result<(), String> {
    use lame_sys::{
        lame_init, lame_set_num_channels, lame_set_in_samplerate, lame_set_quality,
        lame_init_params, lame_encode_buffer_interleaved_ieee_float,
        lame_encode_flush_nogap, lame_close,
    };

    if blob.audio_bytes.is_empty() {
        return Err("No audio bytes in blob — cannot write MP3".into());
    }

    let pcm = pcm_bytes_to_f32(&blob.audio_bytes);
    // Samples per channel (LAME interleaved API takes frames, not total samples)
    let num_samples_per_channel = (pcm.len() / blob.channels.max(1) as usize) as i32;

    // ── Initialise LAME context ──────────────────────────────────────────
    // SAFETY: lame_sys wraps a C library. All pointers are valid for the scope.
    // The gfp context is created, configured, used for encoding, then closed.
    let gfp = unsafe { lame_init() };
    if gfp.is_null() {
        return Err("MP3: lame_init() returned NULL — LAME init failed".into());
    }

    // Wrap in a guard so lame_close is always called even on error
    struct LameGuard(lame_sys::lame_t);
    impl Drop for LameGuard {
        fn drop(&mut self) { unsafe { lame_close(self.0); } }
    }
    let _guard = LameGuard(gfp);

    unsafe {
        let r = lame_set_num_channels(gfp, blob.channels as i32);
        if r < 0 { return Err(format!("MP3: lame_set_num_channels failed: {r}")); }

        let r = lame_set_in_samplerate(gfp, blob.sample_rate as i32);
        if r < 0 { return Err(format!("MP3: lame_set_in_samplerate failed: {r}")); }

        // Quality 2: near-lossless for mastering (0=highest quality, 9=lowest)
        let r = lame_set_quality(gfp, 2);
        if r < 0 { return Err(format!("MP3: lame_set_quality failed: {r}")); }

        let r = lame_init_params(gfp);
        if r < 0 { return Err(format!("MP3: lame_init_params failed: {r}")); }
    }

    // ── Encode interleaved PCM ───────────────────────────────────────────
    // Output buffer: LAME guarantees at most 1.25 * num_samples + 7200 bytes.
    let mp3_buf_size = (1.25 * pcm.len() as f64 + 7200.0) as usize;
    let mut mp3_buf: Vec<u8> = vec![0u8; mp3_buf_size];
    let mut mp3_out: Vec<u8> = Vec::with_capacity(mp3_buf_size);

    let encoded_len = unsafe {
        lame_encode_buffer_interleaved_ieee_float(
            gfp,
            pcm.as_ptr(),
            num_samples_per_channel,
            mp3_buf.as_mut_ptr(),
            mp3_buf_size as i32,
        )
    };
    if encoded_len < 0 {
        return Err(format!("MP3: lame_encode_buffer_interleaved_ieee_float failed: {encoded_len}"));
    }
    mp3_out.extend_from_slice(&mp3_buf[..encoded_len as usize]);

    // ── Flush remaining frames (no decoder delay padding) ───────────────
    let flushed_len = unsafe {
        lame_encode_flush_nogap(
            gfp,
            mp3_buf.as_mut_ptr(),
            mp3_buf_size as i32,
        )
    };
    if flushed_len < 0 {
        return Err(format!("MP3: lame_encode_flush_nogap failed: {flushed_len}"));
    }
    mp3_out.extend_from_slice(&mp3_buf[..flushed_len as usize]);

    tracing::info!(
        path  = %path.display(),
        bytes = mp3_out.len(),
        "m0d: MP3 export complete (LAME dynamic — LGPL compliant)"
    );

    std::fs::write(path, &mp3_out)
        .map_err(|e| format!("MP3: write failed: {e}"))
}


/// Decode f32 LE PCM bytes to sample slice.
/// Samples stored as raw IEEE 754 little-endian floats, 4 bytes per sample.
fn pcm_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

// ── P10-003: Sidecar JSON ─────────────────────────────────────────────────────

/// Sidecar metadata written alongside the audio file as `<name>.stillair.json`.
/// Contains Golden Blob metrics — no audio bytes, no re-measurement.
#[derive(Serialize)]
pub struct ExportSidecar<'a> {
    pub blob_id:       &'a str,
    pub preset_id:     &'a str,
    pub export_format: &'a str,
    pub exported_at:   String,
    pub loudness:      &'a StoredLoudness,
    pub quality:       &'a StoredQuality,
    pub compliance:    ComplianceSummary,
}

#[derive(Serialize)]
pub struct ComplianceSummary {
    pub spotify:   bool,
    pub youtube:   bool,
    pub apple:     bool,
    pub tidal:     bool,
    pub broadcast: bool,
}

/// Write sidecar JSON alongside audio file.
/// Path: audio_path with extension replaced by "stillair.json".
pub fn write_sidecar(blob: &StoredBlob, format: &str, audio_path: &Path) -> Result<(), String> {
    let sidecar_path = audio_path.with_extension("stillair.json");

    let sidecar = ExportSidecar {
        blob_id:       &blob.id,
        preset_id:     &blob.preset_id,
        export_format: format,
        exported_at:   Utc::now().to_rfc3339(),
        loudness:      &blob.loudness,
        quality:       &blob.quality,
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format_from_str() {
        assert!(matches!(ExportFormat::from_str("wav"),  Ok(ExportFormat::Wav)));
        assert!(matches!(ExportFormat::from_str("flac"), Ok(ExportFormat::Flac)));
        assert!(matches!(ExportFormat::from_str("opus"), Ok(ExportFormat::Opus)));
        assert!(matches!(ExportFormat::from_str("mp3"),  Ok(ExportFormat::Mp3)));
        assert!(matches!(ExportFormat::from_str("aiff"), Ok(ExportFormat::Aiff)));
        assert!(matches!(ExportFormat::from_str("aif"),  Ok(ExportFormat::Aiff)));
        assert!(matches!(ExportFormat::from_str("WAV"),  Ok(ExportFormat::Wav)));
        assert!(ExportFormat::from_str("aac").is_err());
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::Wav.extension(),  "wav");
        assert_eq!(ExportFormat::Flac.extension(), "flac");
        assert_eq!(ExportFormat::Opus.extension(), "opus");
        assert_eq!(ExportFormat::Mp3.extension(),  "mp3");
        assert_eq!(ExportFormat::Aiff.extension(), "aiff");
    }

    #[test]
    fn test_f64_to_80bit_extended_zero() {
        let bytes = f64_to_80bit_extended(0.0);
        assert_eq!(bytes, [0u8; 10]);
    }

    #[test]
    fn test_f64_to_80bit_extended_48000() {
        // 48000 Hz = 0xBB80 = 1011 1011 1000 0000
        // exp: biased double = 1023 + 15 = 1038 (0x40E) -> 80-bit: 16383 + 15 = 16398 (0x400E)
        let bytes = f64_to_80bit_extended(48000.0);
        // exponent word: 0x400E (sign=0, exp_80=0x400E)
        assert_eq!(bytes[0], 0x40);
        assert_eq!(bytes[1], 0x0E);
        // Mantissa must have leading bit set (explicit integer bit)
        assert!(bytes[2] & 0x80 != 0);
    }

    #[test]
    fn test_pcm_bytes_roundtrip() {
        let samples = vec![0.5f32, -0.5f32, 1.0f32, 0.0f32];
        let bytes: Vec<u8> = samples.iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let decoded = pcm_bytes_to_f32(&bytes);
        assert_eq!(decoded.len(), samples.len());
        for (a, b) in decoded.iter().zip(samples.iter()) {
            assert!((a - b).abs() < 1e-7);
        }
    }

    #[test]
    fn test_pcm_bytes_empty() {
        let decoded = pcm_bytes_to_f32(&[]);
        assert!(decoded.is_empty());
    }
}
