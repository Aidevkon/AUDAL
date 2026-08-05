use crate::agents::operator::{Intent, MasteringParams};
use crate::app_state::AppState;
use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use std::time::Duration;

/// GET /album/:batch_id/events/stream
pub async fn stream_album_events(
    Path(_batch_id): Path<String>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.album_tx.subscribe();

    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            yield Ok(Event::default().data(json));
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumMasterRequest {
    pub audio_paths: Vec<String>,
    pub preset_id: String,
    pub flavour_id: Option<String>,
    pub intent_tone: Option<f32>,
    pub intent_dynamics: Option<f32>,
}

/// POST /album/master — dispatch a multi-track album cohesion run.
/// Fire-and-forget: returns batch_id immediately, real-time progress
/// via GET /album/:batch_id/events/stream (album_tx broadcast —
/// note: currently unfiltered by batch_id, harmless today since the
/// Conductor only allows one batch in flight at a time, see NEST).
pub async fn trigger_album_mastering(
    State(state): State<AppState>,
    Json(req): Json<AlbumMasterRequest>,
) -> Json<serde_json::Value> {
    let batch_id = uuid::Uuid::new_v4().to_string();

    let items: Vec<MasteringParams> = req
        .audio_paths
        .into_iter()
        .enumerate()
        .map(|(i, audio_path)| MasteringParams {
            audio_path,
            preset_id: req.preset_id.clone(),
            target_lufs: -14.0, // ignored — cohesion pre-pass computes the real per-track target
            max_tp_db: -1.0,    // ignored — v3 resolves this from the preset internally
            session_id: format!("{batch_id}-track-{i}"),
            project_id: None,
            track_id: None,
            flavour_id: req.flavour_id.clone(),
            intent_tone: req.intent_tone,
            intent_dynamics: req.intent_dynamics,
            chaos_seed: None,
            mix_levels: None,
            normalizer_ceiling_db: None,
        })
        .collect();

    let (tx, rx) = tokio::sync::oneshot::channel();
    let intent = Intent::ExecuteBatchMastering {
        batch_id: batch_id.clone(),
        items,
        response: tx,
    };

    tokio::spawn(async move {
        if state.operator.dispatch(intent).await.is_err() {
            tracing::error!("album batch dispatch failed: conductor channel closed");
            return;
        }
        match rx.await {
            Ok(Ok(outputs)) => {
                let ok_count = outputs.iter().filter(|o| o.status == "ok").count();
                tracing::info!("album batch complete: {ok_count}/{} ok", outputs.len());
            }
            Ok(Err(e)) => tracing::error!("album batch failed: {e:?}"),
            Err(_) => tracing::error!("album batch: conductor dropped response"),
        }
    });

    // Return batch_id IMMEDIATELY — HTTP does not wait for the album
    Json(serde_json::json!({ "batch_id": batch_id }))
}
