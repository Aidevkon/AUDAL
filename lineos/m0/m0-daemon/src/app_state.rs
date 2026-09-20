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
use crate::db::DbConn;
use crate::handlers::preview::PreviewStore;
use crate::realtime_bridge::RealtimeBridge;
use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use xaak::engine::PlaybackHandle;
use xaak::playback::ScrubState;
use xaak::repo::{AudioRepo, DspState};

// Moved to conformance 21/09 — a clean type that lived here only
// next to AppState's db/xaak fields (F-137). Re-exported in place so
// every caller (agents/conductor.rs, agents/operator.rs,
// handlers/master.rs, domain/dsp_pipeline.rs) keeps working unchanged.
pub use conformance::mastering_progress::MasteringProgress;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum AlbumEvent {
    #[serde(rename = "pre_analysis")]
    PreAnalysis {
        track: usize,
        bpm: f32,
        ducking_gain: f32,
    },
    #[serde(rename = "forensic")]
    Forensic { track: usize, lufs: f32 },
    #[serde(rename = "cohesion")]
    Cohesion { per_track_targets: Vec<f32> },
    #[serde(rename = "fatigue")]
    Fatigue {
        track: usize,
        ducking: f32,
        width: f32,
    },
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
    pub album_tx: broadcast::Sender<AlbumEvent>,
    pub config: std::sync::Arc<crate::config::M0Config>,
}

impl AppState {
    pub async fn new(
        audit: Arc<AuditLog>,
        config: std::sync::Arc<crate::config::M0Config>,
    ) -> (Self, crate::agents::operator::AgentHandles) {
        // Initialize SurrealDB — persistent local storage
        // ~/.creator_os/db survives reboots (Privacy Moat)
        let db_path = &config.db_path;
        std::fs::create_dir_all(db_path).unwrap_or_default();
        let db = crate::db::init(db_path)
            .await
            .expect("Failed to initialize SurrealDB");

        Self::from_db(audit, db, config).await
    }

    pub async fn new_for_test(
        audit: Arc<AuditLog>,
        config: std::sync::Arc<crate::config::M0Config>,
    ) -> (Self, crate::agents::operator::AgentHandles) {
        let db = crate::db::init_test()
            .await
            .expect("Failed to initialize in-memory test DB");

        Self::from_db(audit, db, config).await
    }

    async fn from_db(
        audit: Arc<AuditLog>,
        db: DbConn,
        config: std::sync::Arc<crate::config::M0Config>,
    ) -> (Self, crate::agents::operator::AgentHandles) {
        let (progress_tx, _) = broadcast::channel(128);
        let (album_tx, _) = broadcast::channel(64);
        let initial_dsp_state = DspState::default();
        let audio_repo = AudioRepo::new_with_flavours(initial_dsp_state);
        let head_state_ptr = Arc::new(ArcSwap::from_pointee(initial_dsp_state));
        let audio_repo_arc = Arc::new(RwLock::new(audio_repo));

        crate::db::schema::migrate(&db)
            .await
            .unwrap_or_else(|e| tracing::warn!("DB migrate: {}", e));

        let blob_store = BlobStore::new();
        let progress_map = Arc::new(DashMap::new());
        let (operator, handles) = crate::agents::operator::spawn_agents(
            audit.clone(),
            head_state_ptr.clone(),
            db.clone(),
            blob_store.clone(),
            album_tx.clone(),
            progress_tx.clone(),
            progress_map.clone(),
            config.clone(),
        );

        let state = Self {
            audit,
            blob_store,
            playback: PlaybackHandle::spawn(),
            progress: progress_map,
            operator,
            preview_store: PreviewStore::new(),
            progress_tx,
            realtime: RealtimeBridge::new(),
            audio_repo: audio_repo_arc,
            head_state_ptr,
            db,
            playback_state: Arc::new(ArcSwap::from_pointee(ScrubState::new())),
            album_tx,
            config,
        };

        (state, handles)
    }
}
