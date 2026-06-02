//! Tauri command: export_audio — P10-005
//! Authority: Phase 10 task-decomposition P10-005 + P10-007
//! Amendment A-002 §3: export goes through Tauri IPC → M0. Never direct from frontend.
//!
//! Flow:
//!   Cockpit FM5 EXPORT button
//!       → export_audio(blob_id, format, app)
//!       → native save dialog (tauri-plugin-dialog)
//!       → POST /export (M0 handler)
//!       → file written to filesystem
//!       → ExportResult { written_path, format } returned to FM5
//!
//! FORBIDDEN:
//!   ❌ Returning raw audio bytes to the frontend
//!   ❌ Re-running DSP or re-measuring loudness
//!   ❌ FFmpeg or any subprocess
//!   ❌ Export command in any state other than FM5

use tauri::command;
use tauri_plugin_dialog::{DialogExt, FilePath};
use serde::{Deserialize, Serialize};
use crate::ipc::m0_client::M0Client;

/// Export result returned to the Cockpit FM5.
/// Contains only path metadata — never audio bytes.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportResult {
    pub written_path: String,
    pub format:       String,
}

/// Default export directory: ~/Music/StillAir/exports/
/// Falls back through hierarchy if audio_dir not available.
fn default_export_dir() -> std::path::PathBuf {
    dirs::audio_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
        .join("StillAir")
        .join("exports")
}

/// Tauri command: export Golden Blob audio via native save dialog.
///
/// 1. Open native save dialog (tauri-plugin-dialog) — user picks path + format
/// 2. POST to M0 /export → file written to disk
/// 3. Return ExportResult to FM5 Cockpit
///
/// Non-fatal errors: return Err, Cockpit stays in FM5 (not FM-ERR).
#[command]
pub async fn export_audio(
    blob_id: String,
    format:  String,    // "wav" | "flac" | "opus"
    app:     tauri::AppHandle,
    client:  tauri::State<'_, M0Client>,
) -> Result<ExportResult, String> {
    eprintln!("[export_audio] called: blob_id={blob_id}, format={format}");

    let extension = format.to_lowercase();
    let default_name = format!("mastered.{extension}");
    let default_dir  = default_export_dir();

    // Ensure default dir exists (non-fatal if fails)
    let _ = std::fs::create_dir_all(&default_dir);

    // Open native save dialog — user picks output path
    let path = tokio::task::spawn_blocking({
        let app  = app.clone();
        let ext  = extension.clone();
        let name = default_name.clone();
        let dir  = default_dir.clone();
        move || {
            app.dialog()
                .file()
                .set_file_name(&name)
                .set_directory(&dir)
                .add_filter(&ext.to_uppercase(), &[&ext])
                .blocking_save_file()
        }
    }).await
    .map_err(|e| format!("Dialog spawn failed: {e}"))?;

    let file_path = match path {
        Some(p) => p,
        None    => {
            eprintln!("[export_audio] dialog returned None — user cancelled");
            return Err("Export cancelled by user".into());
        }
    };
    let output_path = match file_path {
        FilePath::Path(p) => p.to_string_lossy().to_string(),
        FilePath::Url(u)  => u.to_string(),
    };
    eprintln!("[export_audio] dialog path: {output_path}");

    // Call M0 /export — audio bytes written to disk by M0, path returned
    eprintln!("[export_audio] calling M0 export: blob_id={blob_id}, format={format}, path={output_path}");
    let resp = client.export(&blob_id, &format, &output_path)
        .await
        .map_err(|e| format!("IO_ERR:0x02:Export failed: {e}"))?;

    eprintln!("[export_audio] M0 response: status={}, written={:?}, msg={:?}",
        resp.status, resp.written_path, resp.message);

    if resp.status != "ok" {
        return Err(resp.message.unwrap_or_else(|| "export failed".into()));
    }

    Ok(ExportResult {
        written_path: resp.written_path,
        format,
    })
}
