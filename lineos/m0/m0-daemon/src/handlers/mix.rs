//! mix.rs — Audio Git HTTP handlers.
//! /mix/state, /mix/commit, /mix/checkout, /mix/branch, /mix/revert
//! Authority: audio-git-spec-v1_0.md v1.2

use crate::app_state::AppState;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use xaak::repo::DspState;

#[derive(Deserialize)]
pub struct CommitRequest {
    pub message: String,
    pub ducking_depth: Option<f32>,
    pub sidechain_hold: Option<usize>,
    pub ms_width: Option<f32>,
    pub lfe_gain: Option<f32>,
}

#[derive(Deserialize)]
pub struct CheckoutRequest {
    pub branch: String,
}

#[derive(Deserialize)]
pub struct BranchRequest {
    pub name: String,
}

#[derive(Serialize)]
pub struct MixStateResponse {
    pub active_branch: String,
    pub ducking_depth: f32,
    pub sidechain_hold: usize,
    pub ms_width: f32,
    pub lfe_gain: f32,
    pub branches: Vec<String>,
}

#[derive(Serialize)]
pub struct MixResponse {
    pub ok: bool,
    pub message: String,
}

pub async fn get_state(State(app): State<AppState>) -> Json<MixStateResponse> {
    let repo = app.audio_repo.read().unwrap();
    let state = app.head_state_ptr.load_full();
    let branches: Vec<String> = repo.branches.keys().cloned().collect();
    Json(MixStateResponse {
        active_branch: repo.active_branch.clone(),
        ducking_depth: state.ducking_depth,
        sidechain_hold: state.sidechain_hold,
        ms_width: state.ms_width,
        lfe_gain: state.lfe_gain,
        branches,
    })
}

pub async fn post_commit(
    State(app): State<AppState>,
    Json(req): Json<CommitRequest>,
) -> Json<MixResponse> {
    let current = app.head_state_ptr.load_full();
    let new_state = DspState {
        ducking_depth: req.ducking_depth.unwrap_or(current.ducking_depth),
        sidechain_hold: req.sidechain_hold.unwrap_or(current.sidechain_hold),
        ms_width: req.ms_width.unwrap_or(current.ms_width),
        lfe_gain: req.lfe_gain.unwrap_or(current.lfe_gain),
    };
    let hash = app
        .audio_repo
        .write()
        .unwrap()
        .commit(new_state, &req.message);
    app.head_state_ptr.store(Arc::new(new_state));
    Json(MixResponse {
        ok: true,
        message: hash,
    })
}

pub async fn post_checkout(
    State(app): State<AppState>,
    Json(req): Json<CheckoutRequest>,
) -> Json<MixResponse> {
    let result = app.audio_repo.write().unwrap().checkout(&req.branch);
    match result {
        Ok(_) => {
            let state = app.audio_repo.read().unwrap().head_state();
            app.head_state_ptr.store(state);
            Json(MixResponse {
                ok: true,
                message: req.branch,
            })
        }
        Err(e) => Json(MixResponse {
            ok: false,
            message: e,
        }),
    }
}

pub async fn post_branch(
    State(app): State<AppState>,
    Json(req): Json<BranchRequest>,
) -> Json<MixResponse> {
    match app.audio_repo.write().unwrap().create_branch(&req.name) {
        Ok(_) => Json(MixResponse {
            ok: true,
            message: req.name,
        }),
        Err(e) => Json(MixResponse {
            ok: false,
            message: e,
        }),
    }
}

pub async fn post_revert(State(app): State<AppState>) -> Json<MixResponse> {
    let result = app.audio_repo.write().unwrap().revert_head();
    match result {
        Ok(_) => {
            let state = app.audio_repo.read().unwrap().head_state();
            app.head_state_ptr.store(state);
            Json(MixResponse {
                ok: true,
                message: "Reverted".into(),
            })
        }
        Err(e) => Json(MixResponse {
            ok: false,
            message: e,
        }),
    }
}

#[derive(Serialize)]
pub struct FlavourInfo {
    pub name: String,
    pub label: String,
}

#[derive(Serialize)]
pub struct FlavoursResponse {
    pub active: String,
    pub flavours: Vec<FlavourInfo>,
}

pub async fn get_flavours(State(app): State<AppState>) -> Json<FlavoursResponse> {
    let repo = app.audio_repo.read().unwrap();
    let active = repo.active_branch.clone();
    let flavours = xaak::flavours::ALL
        .iter()
        .map(|(name, _)| FlavourInfo {
            name: name.to_string(),
            label: xaak::flavours::label(name).to_string(),
        })
        .collect();
    Json(FlavoursResponse { active, flavours })
}

#[derive(Deserialize)]
pub struct FlavourRequest {
    pub name: String,
}

pub async fn post_flavour(
    State(app): State<AppState>,
    Json(req): Json<FlavourRequest>,
) -> Json<MixResponse> {
    // Validate flavour name
    if xaak::flavours::from_name(&req.name).is_none() {
        return Json(MixResponse {
            ok: false,
            message: format!("Unknown flavour: {}", req.name),
        });
    }
    let result = app.audio_repo.write().unwrap().checkout(&req.name);
    match result {
        Ok(_) => {
            let state = app.audio_repo.read().unwrap().head_state();
            app.head_state_ptr.store(state);
            Json(MixResponse {
                ok: true,
                message: req.name,
            })
        }
        Err(e) => Json(MixResponse {
            ok: false,
            message: e,
        }),
    }
}
