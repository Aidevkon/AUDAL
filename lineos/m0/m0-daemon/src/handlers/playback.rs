use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::app_state::AppState;
use xaak::playback::PlaybackState;

#[derive(Deserialize)]
pub struct SeekRequest {
    pub position_ms: u64,
}

#[derive(Deserialize)]
pub struct PlayRequest {
    pub position_ms: Option<u64>,
}

#[derive(Serialize)]
pub struct PlaybackResponse {
    pub ok:          bool,
    pub position_ms: u64,
    pub playing:     bool,
}

pub async fn post_seek(
    State(app): State<AppState>,
    Json(req):  Json<SeekRequest>,
) -> Json<PlaybackResponse> {
    app.playback_state.store(Arc::new(PlaybackState::paused_at(req.position_ms)));
    Json(PlaybackResponse { ok: true, position_ms: req.position_ms, playing: false })
}

pub async fn post_play(
    State(app): State<AppState>,
    Json(req):  Json<PlayRequest>,
) -> Json<PlaybackResponse> {
    let current = app.playback_state.load_full();
    let pos = req.position_ms.unwrap_or(current.position_ms);
    app.playback_state.store(Arc::new(PlaybackState::playing_at(pos)));
    Json(PlaybackResponse { ok: true, position_ms: pos, playing: true })
}

pub async fn post_pause(State(app): State<AppState>) -> Json<PlaybackResponse> {
    let current = app.playback_state.load_full();
    app.playback_state.store(Arc::new(PlaybackState::paused_at(current.position_ms)));
    Json(PlaybackResponse { ok: true, position_ms: current.position_ms, playing: false })
}

pub async fn get_playback(State(app): State<AppState>) -> Json<PlaybackResponse> {
    let state = app.playback_state.load_full();
    Json(PlaybackResponse { ok: true, position_ms: state.position_ms, playing: state.playing })
}
