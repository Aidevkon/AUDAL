//! AppState — shared state for m0d Axum handlers.
//! Thread-safe, Clone. Passed to all mastering/blob/export handlers.
//! Authority: Phase 6 task-decomposition P6-003

use crate::audit::AuditLog;
use crate::blob_store::BlobStore;
use std::sync::Arc;

/// Shared application state for the mastering API router (port 7400).
#[derive(Clone)]
pub struct AppState {
    pub audit:      Arc<AuditLog>,
    pub blob_store: BlobStore,
}

impl AppState {
    pub fn new(audit: Arc<AuditLog>) -> Self {
        Self {
            audit,
            blob_store: BlobStore::new(),
        }
    }
}
