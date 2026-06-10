//! Tauri command — real-time telemetry from UDP bridge.
//! Uses local RealtimeFrame (not lineos-types).

use crate::telemetry_listener::{LatestFrame, RealtimeFrame};
use tauri::State;

#[tauri::command]
pub fn get_live_telemetry_realtime(latest: State<'_, LatestFrame>) -> Option<RealtimeFrame> {
    latest.lock().ok()?.take()
}
