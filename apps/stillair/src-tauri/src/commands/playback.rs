//! commands/playback.rs — Tauri commands: playback_control, get_playback_state
//! Authority: Phase 12A P12A-007 · Amendment A-003 §8
//!
//! Exposes player control to the Cockpit via IPC.
//! No PCM crosses the IPC boundary — only PlaybackStateJson (A-003 §2).

use crate::ipc::m0_client::M0Client;

/// Playback state visible to the Cockpit — metrics only, no PCM (A-003 §2).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlaybackStateJson {
    pub blob_id:     String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub is_playing:  bool,
    pub sample_rate: u32,
    pub channels:    u16,
}

/// Control playback.
///
/// action: "play" | "pause" | "stop" | "seek"
/// position_ms: required for "seek", ignored otherwise.
///
/// Arg keys: camelCase (Tauri v2 IPC convention — A-003 §7)
#[tauri::command]
pub async fn playback_control(
    action:      String,
    position_ms: Option<u64>,
) -> Result<Option<PlaybackStateJson>, String> {
    let client = M0Client::new();
    client.playback_control(&action, position_ms)
        .await
        .map_err(|e| format!("IO_ERR:0x02:Playback failed: {e}"))
}

/// Get current playback position and state (non-blocking).
#[tauri::command]
pub async fn get_playback_state() -> Result<Option<PlaybackStateJson>, String> {
    let client = M0Client::new();
    client.get_playback_state()
        .await
        .map_err(|e| format!("IO_ERR:0x02:Get playback state failed: {e}"))
}
