//! handlers/playback.rs — POST /playback/control, GET /playback/state
//! Authority: Phase 12A P12A-007 · Amendment A-003 §8
//!
//! PlaybackEngine lives in AppState (Arc<Mutex<PlaybackEngine>>).
//! These handlers expose play/pause/stop/seek and state query.
//! No PCM crosses the HTTP boundary — PlaybackStateResponse is metrics only.

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

/// PlaybackState re-exported for JSON response — no PCM (A-003 §2).
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
    let result = {
        let mut engine = match state.playback.lock() {
            Ok(e)  => e,
            Err(_) => return Json(PlaybackControlResponse {
                status:  "error",
                state:   None,
                message: Some("playback engine lock poisoned".into()),
            }),
        };

        match req.action.as_str() {
            "play"  => engine.play(),
            "pause" => { engine.pause(); Ok(()) },
            "stop"  => { engine.stop();  Ok(()) },
            "seek"  => {
                let ms = req.position_ms.unwrap_or(0);
                engine.seek(ms)
            }
            other => Err(format!("unknown action: {other}")),
        }
    };

    match result {
        Ok(()) => {
            let state_snap = state.playback.lock().ok()
                .and_then(|e| e.state());
            Json(PlaybackControlResponse {
                status:  "ok",
                state:   state_snap,
                message: None,
            })
        }
        Err(e) => Json(PlaybackControlResponse {
            status:  "error",
            state:   None,
            message: Some(e),
        }),
    }
}

/// GET /playback/state — current position, duration, is_playing
pub async fn get_playback_state(
    State(state): State<AppState>,
) -> Json<Option<PlaybackState>> {
    let snap = state.playback.lock().ok()
        .and_then(|e| e.state());
    Json(snap)
}
