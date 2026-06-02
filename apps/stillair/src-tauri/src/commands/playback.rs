//! commands/playback.rs — Tauri commands: playback_control, get_playback_state, get_live_telemetry
//! Authority: Phase 12A P12A-007 · Phase 12B P12B-005 · Amendment A-003 §8
//!
//! Exposes player control and live telemetry to the Cockpit via IPC.
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
    #[serde(alias = "ab_target", deserialize_with = "deserialize_active_ab", default = "default_active_ab")]
    pub active_ab:   String,
}

fn default_active_ab() -> String {
    "B".to_string()
}

fn deserialize_active_ab<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = serde::Deserialize::deserialize(deserializer).unwrap_or(None);
    Ok(match s.as_deref() {
        Some("a") | Some("A") => "A".to_string(),
        _ => "B".to_string(),
    })
}

/// Live telemetry — momentary LUFS + short-term during playback (P12B-005).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveTelemetryJson {
    pub momentary_lufs:  f32,
    pub short_term_lufs: f32,
    pub true_peak_dbtp:  f32,
    pub position_ms:     u64,
}

/// Control playback.
///
/// action: "play" | "pause" | "stop" | "seek"
/// position_ms: required for "seek", ignored otherwise.
#[tauri::command]
pub async fn playback_control(
    action:      String,
    position_ms: Option<u64>,
    client:      tauri::State<'_, M0Client>,
) -> Result<Option<PlaybackStateJson>, String> {
    client.playback_control(&action, position_ms)
        .await
        .map_err(|e| format!("IO_ERR:0x02:Playback failed: {e}"))
}

/// Get current playback position and state (non-blocking).
#[tauri::command]
pub async fn get_playback_state(
    client: tauri::State<'_, M0Client>,
) -> Result<Option<PlaybackStateJson>, String> {
    client.get_playback_state()
        .await
        .map_err(|e| format!("IO_ERR:0x02:Get playback state failed: {e}"))
}

/// Get live telemetry during playback (P12B-005).
/// Returns momentary LUFS from the active blob's stored metrics.
/// Uses playback position to select the correct telemetry window.
#[tauri::command]
pub async fn get_live_telemetry(
    client: tauri::State<'_, M0Client>,
) -> Result<Option<LiveTelemetryJson>, String> {
    client.get_live_telemetry()
        .await
        .map_err(|e| format!("IO_ERR:0x02:Live telemetry failed: {e}"))
}
