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
use std::sync::Arc;
use xaak::engine::PlaybackHandle;

/// Shared application state for the mastering API router (port 7400).
#[derive(Clone)]
pub struct AppState {
    pub audit:      Arc<AuditLog>,
    pub blob_store: BlobStore,
    /// Send-safe handle to the xaak playback worker thread (A-003 §1).
    pub playback:   PlaybackHandle,
}

impl AppState {
    pub fn new(audit: Arc<AuditLog>) -> Self {
        Self {
            audit,
            blob_store: BlobStore::new(),
            playback:   PlaybackHandle::spawn(),
        }
    }
}
