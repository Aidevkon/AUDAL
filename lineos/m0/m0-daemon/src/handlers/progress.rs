use axum::{extract::{Path, State}, Json};
use crate::app_state::{AppState, MasteringProgress};

pub async fn get_progress(
    Path(job_id): Path<String>,
    State(state): State<AppState>,
) -> Json<MasteringProgress> {
    match state.progress.get(&job_id) {
        Some(p) => Json(p.clone()),
        None    => Json(MasteringProgress {
            job_id, stage: "UNKNOWN".into(),
            elapsed_ms: 0, blob_id: None, error: None,
        }),
    }
}
