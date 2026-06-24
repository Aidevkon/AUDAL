use crate::app_state::AppState;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use xaak::playback::ScrubState;

#[derive(Deserialize)]
pub struct PlaybackControlRequest {
    pub action: String,
    pub position_ms: Option<u64>,
}

#[derive(Serialize)]
pub struct ControlResp {
    pub status: String,
    pub state: Option<xaak::PlaybackState>,
    pub message: Option<String>,
}

pub async fn post_control(
    State(app): State<AppState>,
    Json(req): Json<PlaybackControlRequest>,
) -> Json<ControlResp> {
    // 1. Send command to xaak engine
    match req.action.as_str() {
        "play" => {
            app.playback.play();
            if let Some(pos) = req.position_ms {
                app.playback_state
                    .store(Arc::new(ScrubState::playing_at(pos)));
            } else {
                let current = app.playback_state.load_full().position_ms;
                app.playback_state
                    .store(Arc::new(ScrubState::playing_at(current)));
            }
        }
        "pause" => {
            app.playback.pause();
            let current = app.playback_state.load_full().position_ms;
            app.playback_state
                .store(Arc::new(ScrubState::paused_at(current)));
        }
        "stop" => {
            app.playback.stop();
            app.playback_state.store(Arc::new(ScrubState::paused_at(0)));
        }
        "seek" => {
            if let Some(pos) = req.position_ms {
                app.playback.seek(pos);
                let current_state = app.playback_state.load_full();
                if current_state.playing {
                    app.playback_state
                        .store(Arc::new(ScrubState::playing_at(pos)));
                } else {
                    app.playback_state
                        .store(Arc::new(ScrubState::paused_at(pos)));
                }
            }
        }
        _ => {}
    }

    // 2. Return xaak engine state to UI
    let state = app.playback.get_state();
    Json(ControlResp {
        status: "ok".to_string(),
        state,
        message: None,
    })
}

pub async fn get_state(State(app): State<AppState>) -> Json<ControlResp> {
    let state = app.playback.get_state();
    Json(ControlResp {
        status: "ok".to_string(),
        state,
        message: None,
    })
}
