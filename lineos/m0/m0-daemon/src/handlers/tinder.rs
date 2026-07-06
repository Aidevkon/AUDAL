//! tinder.rs — Mastering Tinder HTTP handlers.
//! FL-T2: GET  /tinder/variations → 8 DspState options
//! FL-T3: POST /tinder/like       → record preference
//! FL-T4: POST /tinder/result     → weighted centroid → "my_sound"

use crate::app_state::AppState;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use xaak::flavours;
use xaak::repo::DspState;
use xaak::tinder::{generate_variations, weighted_centroid};

const N_VARIATIONS: usize = 8;

#[derive(Serialize)]
pub struct VariationInfo {
    pub idx: usize,
    pub ducking_depth: f32,
    pub ms_width: f32,
    pub lfe_gain: f32,
    pub sidechain_hold: usize,
    pub label: String,
}

#[derive(Serialize)]
pub struct VariationsResponse {
    pub variations: Vec<VariationInfo>,
    pub current: VariationInfo,
}

#[derive(Deserialize)]
pub struct LikeRequest {
    pub variation_idx: usize,
}

#[derive(Serialize)]
pub struct LikeResponse {
    pub ok: bool,
    pub liked: usize,
    pub message: String,
}

#[derive(Serialize)]
pub struct ResultResponse {
    pub ok: bool,
    pub branch: String,
    pub ducking_depth: f32,
    pub ms_width: f32,
    pub lfe_gain: f32,
    pub liked_count: usize,
}

fn state_to_info(idx: usize, s: &DspState) -> VariationInfo {
    // Match to closest flavour label
    let label = flavours::ALL
        .iter()
        .min_by(|(_, a), (_, b)| {
            let da = (a.ducking_depth - s.ducking_depth).abs() + (a.ms_width - s.ms_width).abs();
            let db = (b.ducking_depth - s.ducking_depth).abs() + (b.ms_width - s.ms_width).abs();
            da.total_cmp(&db)
        })
        .map(|(name, _)| flavours::label(name).to_string())
        .unwrap_or_else(|| "Custom".to_string());

    VariationInfo {
        idx,
        ducking_depth: s.ducking_depth,
        ms_width: s.ms_width,
        lfe_gain: s.lfe_gain,
        sidechain_hold: s.sidechain_hold,
        label,
    }
}

/// GET /tinder/variations — generate 8 variations from current state
pub async fn get_variations(State(app): State<AppState>) -> Json<VariationsResponse> {
    let current = app.head_state_ptr.load_full();
    let vars = generate_variations(&current, N_VARIATIONS);
    Json(VariationsResponse {
        current: state_to_info(0, &current),
        variations: vars
            .iter()
            .enumerate()
            .map(|(i, s)| state_to_info(i, s))
            .collect(),
    })
}

/// POST /tinder/like { variation_idx } — store liked variation
pub async fn post_like(
    State(app): State<AppState>,
    Json(req): Json<LikeRequest>,
) -> Json<LikeResponse> {
    let current = app.head_state_ptr.load_full();
    let vars = generate_variations(&current, N_VARIATIONS);

    let Some(liked_state) = vars.get(req.variation_idx) else {
        return Json(LikeResponse {
            ok: false,
            liked: 0,
            message: format!("Invalid variation index: {}", req.variation_idx),
        });
    };

    // Store liked variation as a commit on main
    let msg = format!("Tinder like #{}", req.variation_idx);
    let hash = app.audio_repo.write().unwrap().commit(*liked_state, &msg);
    app.head_state_ptr.store(Arc::new(*liked_state));

    Json(LikeResponse {
        ok: true,
        liked: req.variation_idx,
        message: hash,
    })
}

/// POST /tinder/result — compute weighted centroid of all liked commits
/// → commit as "my_sound" branch
pub async fn post_result(State(app): State<AppState>) -> Json<ResultResponse> {
    // Collect all tinder commits from main branch
    let liked_states: Vec<DspState> = {
        let repo = app.audio_repo.read().unwrap();
        repo.commits
            .values()
            .filter(|c| c.message.starts_with("Tinder like"))
            .map(|c| c.state)
            .collect()
    };

    if liked_states.is_empty() {
        let current = app.head_state_ptr.load_full();
        return Json(ResultResponse {
            ok: false,
            branch: "main".into(),
            ducking_depth: current.ducking_depth,
            ms_width: current.ms_width,
            lfe_gain: current.lfe_gain,
            liked_count: 0,
        });
    }

    let my_sound = weighted_centroid(&liked_states).unwrap();

    // Create "my_sound" branch and commit
    {
        let mut repo = app.audio_repo.write().unwrap();
        let _ = repo.create_branch("my_sound");
        repo.checkout("my_sound").unwrap_or(());
        repo.commit(my_sound, "My Sound — Tinder result");
        let _ = repo.checkout("my_sound");
    }
    app.head_state_ptr.store(Arc::new(my_sound));

    Json(ResultResponse {
        ok: true,
        branch: "my_sound".into(),
        ducking_depth: my_sound.ducking_depth,
        ms_width: my_sound.ms_width,
        lfe_gain: my_sound.lfe_gain,
        liked_count: liked_states.len(),
    })
}
