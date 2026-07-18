//! GET /progress/:job_id        — JSON snapshot (backward compatible)
//! GET /progress/:job_id/stream — SSE event-driven stream
//! Authority: spatial-mixer-widget-v1_0.md §4.6
//!
//! SSE uses tokio::sync::broadcast — zero CPU polling.
//! Worker sends MasteringProgress via state.progress_tx.send().
//! Stream closes automatically on CERTIFIED / BATCH_COMPLETE / ERROR.

use crate::app_state::{AppState, MasteringProgress};
use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use std::time::Duration;

/// GET /progress/:job_id — JSON snapshot (backward compatible).
pub async fn get_progress(
    Path(job_id): Path<String>,
    State(state): State<AppState>,
) -> Json<MasteringProgress> {
    match state.progress.get(&job_id) {
        Some(p) => Json(p.clone()),
        None => Json(MasteringProgress {
            job_id,
            stage: "UNKNOWN".into(),
            elapsed_ms: 0,
            blob_id: None,
            error: None,
            bpm: None,
        }),
    }
}

/// GET /progress/:job_id/stream — SSE event-driven stream.
/// Zero CPU polling — wakes only when a progress event fires.
/// Closes when stage is CERTIFIED, BATCH_COMPLETE, or ERROR.
pub async fn stream_progress(
    Path(job_id): Path<String>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.progress_tx.subscribe();

    let stream = async_stream::stream! {
        // Send current state immediately if available
        if let Some(p) = state.progress.get(&job_id) {
            yield Ok(make_event(&p));
            if is_terminal(&p.stage) { return; }
        } else {
            yield Ok(Event::default().data(
                serde_json::json!({
                    "stage":    "WAITING",
                    "progress": 0,
                    "jobId":    &job_id,
                    "blobId":   null,
                    "error":    null,
                }).to_string()
            ));
        }

        // Event-driven loop — zero CPU when idle
        while let Ok(p) = rx.recv().await {
            if p.job_id == job_id {
                yield Ok(make_event(&p));
                if is_terminal(&p.stage) { break; }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

fn make_event(p: &MasteringProgress) -> Event {
    Event::default().data(
        serde_json::json!({
            "stage":    p.stage,
            "progress": stage_to_percent(&p.stage),
            "jobId":    p.job_id,
            "blobId":   p.blob_id,
            "error":    p.error,
        })
        .to_string(),
    )
}

fn is_terminal(stage: &str) -> bool {
    stage == "CERTIFIED" || stage.starts_with("BATCH_COMPLETE") || stage == "ERROR"
}

fn stage_to_percent(stage: &str) -> u8 {
    match stage {
        "WAITING" => 0,
        "DISPATCHED" => 5,
        "QUEUED" => 5,
        "INITIALIZING" => 10,
        "ANALYZING" => 20,
        "STEMS" => 35,
        "MARKOV" => 45,
        "DSP" => 60,
        "SPATIAL" => 75,
        "MASTERING" => 85,
        "CERTIFIED" => 100,
        "BATCH_STARTED" => 5,
        "ERROR" => 100,
        s if s.starts_with("BATCH_COMPLETE") => 100,
        _ => 50,
    }
}
