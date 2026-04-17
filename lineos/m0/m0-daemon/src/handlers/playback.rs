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
