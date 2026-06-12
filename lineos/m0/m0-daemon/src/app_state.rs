//! AppState — shared state for m0d Axum handlers.
//! Thread-safe, Clone. Passed to all mastering/blob/export handlers.
//! Authority: Phase 6 task-decomposition P6-003
//! Phase 12A: xaak PlaybackHandle added (Amendment A-003 §1)
//!
//! Note: cpal::Stream is !Send, so PlaybackEngine cannot live in AppState.
//! PlaybackHandle wraps a Sender<PlaybackCmd> — Send + Sync — and the
//! actual engine/stream lives on a dedicated worker thread.

use crate::agents::operator::Operator;
use crate::audit::AuditLog;
use crate::blob_store::BlobStore;
use crate::handlers::preview::PreviewStore;
use crate::realtime_bridge::RealtimeBridge;
use dashmap::DashMap;
use arc_swap::ArcSwap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use xaak::engine::PlaybackHandle;
use xaak::repo::{AudioRepo, DspState};
use crate::db::DbConn;
use xaak::playback::ScrubState;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MasteringProgress {
    pub job_id: String,
    pub stage: String,
    pub elapsed_ms: u64,
    pub blob_id: Option<String>,
    pub error: Option<String>,
}

/// Shared application state for the mastering API router (port 7400).
#[derive(Clone)]
pub struct AppState {
    pub audit: Arc<AuditLog>,
    pub blob_store: BlobStore,
    /// Send-safe handle to the xaak playback worker thread (A-003 §1).
    pub playback: PlaybackHandle,
    pub progress: Arc<DashMap<String, MasteringProgress>>,
    /// Constitutional Agent Architecture v3.1 — Intent dispatcher.
    pub operator: Operator,
    /// Phase 8a: preview stem store for 5.1 Spatial Mixer widget.
    pub preview_store: PreviewStore,
    /// Phase 8c: broadcast channel for SSE progress stream.
    /// Workers send MasteringProgress events — SSE streams receive them.
    pub progress_tx: broadcast::Sender<MasteringProgress>,
    /// Phase 9 TB-P2: lock-free ring buffer for xaak → UI telemetry.
    pub realtime: RealtimeBridge,
    /// Audio Git — version control for DSP state.
    /// UI thread writes via RwLock. Audio thread reads via head_state_ptr.
    pub audio_repo: Arc<RwLock<AudioRepo>>,
    /// Lock-free hot pointer for audio thread.
    pub head_state_ptr: Arc<ArcSwap<DspState>>,
    /// Embedded SurrealDB — Projects, Tracks, Sessions.
    /// Privacy moat: 100% local, kv-surrealkv backend.
    pub db: DbConn,
    pub playback_state: Arc<ArcSwap<ScrubState>>,
}

impl AppState {
    pub async fn new(audit: Arc<AuditLog>) -> Self {
        let (progress_tx, _) = broadcast::channel(128);
        let initial_dsp_state = DspState::default();
        let audio_repo = AudioRepo::new_with_flavours(initial_dsp_state.clone());
        let head_state_ptr = Arc::new(ArcSwap::from_pointee(initial_dsp_state));
        let audio_repo_arc = Arc::new(RwLock::new(audio_repo));

        // Initialize SurrealDB — persistent local storage
        // ~/.creator_os/db survives reboots (Privacy Moat)
        let fallback_path = format!(
            "{}/.creator_os/db",
            std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
        );
        let db_path = std::env::var("CREATOR_OS_DB_PATH")
            .unwrap_or(fallback_path);
        std::fs::create_dir_all(&db_path).unwrap_or_default();
        let db = crate::db::init(&db_path).await
            .expect("Failed to initialize SurrealDB");
        crate::db::schema::migrate(&db).await
            .unwrap_or_else(|e| tracing::warn!("DB migrate: {}", e));

        let operator = crate::agents::operator::spawn_agents(audit.clone(), head_state_ptr.clone(), db.clone());

        Self {
            audit,
            blob_store: BlobStore::new(),
            playback: PlaybackHandle::spawn(),
            progress: Arc::new(DashMap::new()),
            operator,
            preview_store: PreviewStore::new(),
            progress_tx,
            realtime: RealtimeBridge::new(),
            audio_repo: audio_repo_arc,
            head_state_ptr,
            db,
            playback_state: Arc::new(ArcSwap::from_pointee(ScrubState::new())),
        }
    }
}
