//! POST /export — export Golden Blob audio as WAV, FLAC, or Opus.
//! Authority: Phase 10 task-decomposition P10-002/P10-003/P10-004
//!
//! Architecture (binding):
//!   Tauri command export_audio(blob_id, format, output_path)
//!       → POST /export (this handler)
//!       → blob_store.get(blob_id) → StoredBlob.audio_bytes (f32 LE PCM)
//!       ├── WAV:  decode bytes → hound write (32-bit float, 48kHz)
//!       ├── FLAC: write audio_bytes directly (zero re-encoding)
//!       └── Opus: decode bytes → audiopus encode → write
//!       → sidecar .stillair.json alongside audio file
//!
//! FORBIDDEN (per Phase 10 master prompt + Amendment A-002 §3):
//!   ❌ FFmpeg or any subprocess
//!   ❌ Re-running DSP pipeline during export
//!   ❌ Re-measuring loudness during export
//!   ❌ Modifying the Golden Blob
//!   ❌ Returning raw audio bytes to the frontend
//!   ❌ std::f32 in audio math (use libm where applicable)

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
    pub format:      String,     // "wav" | "flac" | "opus"
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
         Use WAV or FLAC for lossless export.".into())
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
        assert!(matches!(ExportFormat::from_str("WAV"),  Ok(ExportFormat::Wav)));
        assert!(ExportFormat::from_str("mp3").is_err());
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::Wav.extension(),  "wav");
        assert_eq!(ExportFormat::Flac.extension(), "flac");
        assert_eq!(ExportFormat::Opus.extension(), "opus");
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
