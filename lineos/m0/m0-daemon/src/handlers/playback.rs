//! handlers/playback.rs — POST /playback/control, GET /playback/state
//! Authority: Phase 12A P12A-007 · Amendment A-003 §8
//!
//! PlaybackHandle in AppState is Send+Sync (mpsc Sender).
//! The actual cpal::Stream lives on a dedicated worker thread.
//! No PCM crosses the HTTP boundary — PlaybackState is metrics only.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use xaak::PlaybackState;

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PlaybackControlRequest {
    /// "play" | "pause" | "stop" | "seek"
    pub action:      String,
    /// Required for action="seek"
    pub position_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct PlaybackControlResponse {
    pub status:  &'static str,
    pub state:   Option<PlaybackState>,
    pub message: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /playback/control — play | pause | stop | seek
pub async fn playback_control(
    State(state): State<AppState>,
    Json(req):    Json<PlaybackControlRequest>,
) -> Json<PlaybackControlResponse> {
    let result: Result<(), String> = match req.action.as_str() {
        "play"  => { state.playback.play();  Ok(()) }
        "pause" => { state.playback.pause(); Ok(()) }
        "stop"  => { state.playback.stop();  Ok(()) }
        "seek"  => {
            state.playback.seek(req.position_ms.unwrap_or(0));
            Ok(())
        }
        other => Err(format!("unknown action: {other}")),
    };

    match result {
        Ok(()) => {
            // get_state() is a synchronous round-trip to the worker thread
            let snap = state.playback.get_state();
            Json(PlaybackControlResponse { status: "ok", state: snap, message: None })
        }
        Err(e) => Json(PlaybackControlResponse {
            status:  "error",
            state:   None,
            message: Some(e),
        }),
    }
}

/// GET /playback/state — current position (synchronous round-trip to worker).
pub async fn get_playback_state(
    State(state): State<AppState>,
) -> Json<Option<PlaybackState>> {
    Json(state.playback.get_state())
}

/// GET /playback/telemetry — momentary LUFS from blob metrics (P12B-005).
/// Reads the active blob's stored loudness metrics.
/// Does NOT expose PCM — numbers only (A-003 §5).
pub async fn get_live_telemetry(
    State(state): State<AppState>,
) -> Json<Option<LiveTelemetryResponse>> {
    // Get current playback state to find the active blob_id
    let playback_state = state.playback.get_state();
    let Some(ps) = playback_state else {
        return Json(None);
    };

    // Look up the blob's stored loudness metrics
    let blob = state.blob_store.get(&ps.blob_id);
    let Some(blob) = blob else {
        return Json(None);
    };

    Json(Some(LiveTelemetryResponse {
        momentary_lufs:  blob.loudness.momentary_lufs,
        short_term_lufs: blob.loudness.short_term_lufs,
        true_peak_dbtp:  blob.loudness.true_peak_dbtp,
        position_ms:     ps.position_ms,
    }))
}

#[derive(Debug, serde::Serialize)]
pub struct LiveTelemetryResponse {
    pub momentary_lufs:  f32,
    pub short_term_lufs: f32,
    pub true_peak_dbtp:  f32,
    pub position_ms:     u64,
}
