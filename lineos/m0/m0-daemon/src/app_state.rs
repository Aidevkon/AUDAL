//! AppState — shared state for m0d Axum handlers.
//! Thread-safe, Clone. Passed to all mastering/blob/export handlers.
//! Authority: Phase 6 task-decomposition P6-003
//! Phase 12A: xaak PlaybackHandle added (Amendment A-003 §1)
//!
//! Note: cpal::Stream is !Send, so PlaybackEngine cannot live in AppState.
//! PlaybackHandle wraps a Sender<PlaybackCmd> — Send + Sync — and the
//! actual engine/stream lives on a dedicated worker thread.

use crate::audit::AuditLog;
use crate::blob_store::BlobStore;
use crate::agents::operator::Operator;
use crate::handlers::preview::PreviewStore;
use std::sync::Arc;
use xaak::engine::PlaybackHandle;
use dashmap::DashMap;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MasteringProgress {
    pub job_id:     String,
    pub stage:      String,
    pub elapsed_ms: u64,
    pub blob_id:    Option<String>,
    pub error:      Option<String>,
}

/// Shared application state for the mastering API router (port 7400).
#[derive(Clone)]
pub struct AppState {
    pub audit:         Arc<AuditLog>,
    pub blob_store:    BlobStore,
    /// Send-safe handle to the xaak playback worker thread (A-003 §1).
    pub playback:      PlaybackHandle,
    pub progress:      Arc<DashMap<String, MasteringProgress>>,
    /// Constitutional Agent Architecture v3.1 — Intent dispatcher.
    pub operator:      Operator,
    /// Phase 8a: preview stem store for 5.1 Spatial Mixer widget.
    pub preview_store: PreviewStore,
}

impl AppState {
    pub fn new(audit: Arc<AuditLog>) -> Self {
        let operator = crate::agents::operator::spawn_agents(audit.clone());
        Self {
            audit,
            blob_store:    BlobStore::new(),
            playback:      PlaybackHandle::spawn(),
            progress:      Arc::new(DashMap::new()),
            operator,
            preview_store: PreviewStore::new(),
        }
    }
}
