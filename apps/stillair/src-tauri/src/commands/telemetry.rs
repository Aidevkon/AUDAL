//! Tauri command for real-time telemetry.
//! Authority: telemetry-bridge-spec-v1_3.md TB-P5
//!
//! Reads latest RealtimeFrame from LatestFrame state.
//! Called by Dioxus via requestAnimationFrame at native Hz.
//! INV-TB-6: zero network hop — Tauri IPC shared memory.

use tauri::State;
use crate::telemetry_listener::LatestFrame;
use lineos_types::RealtimeFrame;

/// Pop latest RealtimeFrame decoded from UDP.
/// Returns None if xaak not playing or no frame received yet.
/// Calling this does NOT reset the frame — use peek semantics.
#[tauri::command]
pub fn get_live_telemetry_realtime(
    latest: State<'_, LatestFrame>,
) -> Option<RealtimeFrame> {
    latest.lock().ok()?.take()
}
