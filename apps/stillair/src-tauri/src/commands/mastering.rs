//! Mastering commands — real M0 IPC calls replacing Phase 5 stubs.
//! Authority: Phase 6 task-decomposition P6-004
//! Flow: Cockpit → Tauri → M0 → sp314-dsp → Golden Blob
//!
//! FORBIDDEN: Calling sp314-dsp directly (must go through M0).
//! FORBIDDEN: Audio processing in this module.
//! FORBIDDEN: serde_json::Value in return types.

use tauri::command;
use crate::ipc::m0_client::{GoldenBlobJson, M0Client, MasterRequest};

/// Real mastering: Cockpit → Tauri → M0 → sp314-dsp.
/// Phase 5 replaced: 2s stub gone.
/// Returns blob_id on success; ASC-mapped error string on failure.
#[command]
pub async fn trigger_mastering(
    audio_path: String,
    preset_id:  String,
) -> Result<String, String> {
    let client = M0Client::new();

    // Guard: verify M0 is healthy. WasmPanic (ASC 0x05) guard.
    client.health().await
        .map_err(|e| format!("M0 unreachable: {e}"))?;

    let resp = client
        .trigger_mastering(MasterRequest { audio_path, preset_id })
        .await
        .map_err(|e| e.to_string())?;

    if resp.status != "ok" {
        return Err(resp.message.unwrap_or_else(|| "mastering failed".into()));
    }

    Ok(resp.blob_id)
}

/// Load audio file metadata — called on FM0 → FM1 transition.
/// Phase 5 stub retained for now; Phase 7 wires real M0 decode.
#[command]
pub async fn load_audio_file(path: String) -> Result<crate::commands::AudioMeta, String> {
    let name = path.split('/').last().unwrap_or("unknown").to_string();
    Ok(crate::commands::AudioMeta {
        name,
        format:      "WAV".to_string(),
        sample_rate: 48_000,
        bit_depth:   24,
        duration_s:  180.0,
        channels:    2,
    })
}

/// Fetch Golden Blob as JSON — called on FM2 → FM3 to start Data Cascade.
/// No binary data crosses the IPC boundary.
#[command]
pub async fn get_golden_blob(blob_id: String) -> Result<GoldenBlobJson, String> {
    let client = M0Client::new();
    client.get_blob(&blob_id).await.map_err(|e| e.to_string())
}

/// Audio file metadata returned to frontend on file load.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioMeta {
    pub name:        String,
    pub format:      String,
    pub sample_rate: u32,
    pub bit_depth:   u8,
    pub duration_s:  f32,
    pub channels:    u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_audio_file_stub_returns_meta() {
        let meta = load_audio_file("/tmp/track.wav".into()).await.unwrap();
        assert_eq!(meta.name, "track.wav");
        assert_eq!(meta.sample_rate, 48_000);
        assert_eq!(meta.channels, 2);
    }
}
