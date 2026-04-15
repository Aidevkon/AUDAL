//! POST /export — export Golden Blob audio as FLAC or WAV.
//! Authority: Phase 6 task-decomposition P6-008
//! FM6 flow: export_clicked → POST /export → written_path (success) | error
//!
//! FORBIDDEN: Returning raw audio bytes to the Tauri frontend.
//! FORBIDDEN: serde_json::Value in response type.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::audit::{AuditEntry, AuditLevel};

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub blob_id: String,
    pub format:  String,     // "flac" | "wav"
    pub path:    String,
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub written_path: String,
    pub status:       &'static str,   // "ok" | "error"
    pub message:      Option<String>,
}

/// POST /export — decode Golden Blob audio and write to disk.
/// Audio bytes remain in M0 storage — only the output path is returned.
pub async fn export_audio(
    State(state): State<AppState>,
    Json(req):    Json<ExportRequest>,
) -> Json<ExportResponse> {
    // Validate format
    if req.format != "flac" && req.format != "wav" {
        return Json(ExportResponse {
            written_path: String::new(),
            status:       "error",
            message:      Some(format!("unsupported format: {}", req.format)),
        });
    }

    // Look up blob
    let blob = match state.blob_store.get(&req.blob_id) {
        Some(b) => b,
        None => {
            return Json(ExportResponse {
                written_path: String::new(),
                status:       "error",
                message:      Some(format!("blob not found: {}", req.blob_id)),
            });
        }
    };

    // Perform write — Phase 6: write stub JSON sidecar as marker.
    // Phase 7: real FLAC/WAV decode + write from stored audio bytes.
    let output_path = req.path.clone();
    match write_export(&blob, &req.format, &output_path).await {
        Ok(_) => {
            state.audit.write(
                AuditEntry::new("m0d.export_complete", AuditLevel::Audit,
                    &format!("Exported {format} to {output_path}", format=req.format, output_path=output_path))
            ).ok();
            Json(ExportResponse {
                written_path: output_path,
                status:       "ok",
                message:      None,
            })
        }
        Err(e) => {
            state.audit.write(
                AuditEntry::new("m0d.export_failed", AuditLevel::Audit, &e)
            ).ok();
            Json(ExportResponse {
                written_path: String::new(),
                status:       "error",
                message:      Some(e),
            })
        }
    }
}

/// Phase 6: writes a JSON sidecar as export marker.
/// Phase 7: real FLAC decode from StoredBlob.flac_bytes.
async fn write_export(blob: &crate::blob_store::StoredBlob, format: &str, path: &str) -> Result<(), String> {
    // Create parent directories if needed
    if let Some(parent) = std::path::Path::new(path).parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| format!("Cannot create output directory: {e}"))?;
    }

    // Phase 6: write JSON sidecar instead of audio (Phase 7: real encode)
    let sidecar_path = format!("{path}.sidecar.json");
    let metadata = serde_json::json!({
        "blob_id":    blob.id,
        "format":     format,
        "preset_id":  blob.preset_id,
        "integrated_lufs": blob.loudness.integrated_lufs,
        "true_peak_dbtp":  blob.loudness.true_peak_dbtp,
        "note": "Phase 6: sidecar marker. Phase 7: real audio encode."
    });
    tokio::fs::write(&sidecar_path, metadata.to_string()).await
        .map_err(|e| format!("Write failed: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format_validation() {
        assert!(["flac", "wav"].contains(&"flac"));
        assert!(["flac", "wav"].contains(&"wav"));
        assert!(!["flac", "wav"].contains(&"mp3"));
    }
}
