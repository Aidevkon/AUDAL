//! Export command — FM6 real FLAC/WAV export via M0.
//! Authority: Phase 6 task-decomposition P6-008
//! FM6 flow: export_clicked → FM6 (UI locked) → export_audio
//!           → success: FM5 | error: FM-ERR (ASC 0x02)

use tauri::command;
use crate::ipc::m0_client::M0Client;
use serde::{Deserialize, Serialize};

/// Export result returned to Cockpit.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportResult {
    pub written_path: String,
}

/// Export Golden Blob audio to FLAC or WAV via M0.
/// On success: returns written_path.
/// On failure: returns Err (Cockpit transitions to FM-ERR ASC 0x02).
#[command]
pub async fn export_audio(
    blob_id: String,
    format:  String,   // "flac" | "wav"
    path:    String,
) -> Result<ExportResult, String> {
    let client = M0Client::new();
    let resp = client.export(&blob_id, &format, &path)
        .await
        .map_err(|e| e.to_string())?;

    if resp.status != "ok" {
        return Err(resp.message.unwrap_or_else(|| "export failed".into()));
    }

    Ok(ExportResult { written_path: resp.written_path })
}
