//! Operator — Intent enum + channel ownership + dispatch routing.
//! Authority: Constitutional Agent Architecture Spec v3.1
//!
//! Single entry point for all cross-agent communication.
//! Every Intent is logged before dispatch.
//! No agent calls another agent directly.

use crate::audit::{AuditEntry, AuditLevel, AuditLog};
use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use xaak::repo::DspState;

#[derive(Debug)]
pub enum Intent {
    // R1 — Schema
    ValidateSchema {
        patch: serde_json::Value,
        response: oneshot::Sender<Result<(), SchemaError>>,
    },
    QuerySchema {
        path: String,
        response: oneshot::Sender<serde_json::Value>,
    },
    // R2 — Conductor → full mastering workflow
    ExecuteMastering {
        params: MasteringParams,
        response: oneshot::Sender<Result<MasteringOutput, ConductorError>>,
    },
    // R3 — Conductor → Executor
    RunDsp {
        plan: ExecutionPlan,
        response: oneshot::Sender<Result<DspOutput, ExecutorError>>,
    },
    // R3 — analysis only (decode + PreAnalyzer, no DSP)
    RunAnalysis {
        audio_path: String,
        session_id: String,
        response: oneshot::Sender<Result<AnalysisResult, ExecutorError>>,
    },
    // R2 — Conductor → streaming workflow
    ExecuteStreaming {
        params: StreamingParams,
        response: oneshot::Sender<Result<StreamingOutput, ConductorError>>,
    },
    // R3 — Conductor → Executor (streaming)
    RunStreaming {
        plan: StreamingPlan,
        response: oneshot::Sender<Result<StreamingOutput, ExecutorError>>,
    },
    // R2 — Conductor batch workflow
    ExecuteBatchMastering {
        batch_id: String,
        items: Vec<MasteringParams>,
        response: oneshot::Sender<Result<Vec<BatchTrackOutput>, ConductorError>>,
    },
    // R2 — WizardAgent
    AnalyzeTelemetry {
        metrics: AudioMetrics,
        response: oneshot::Sender<Vec<Finding>>,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum SchemaError {
    ValidationFailed(String),
    PathNotFound(String),
}

#[derive(Debug, Clone)]
pub enum ConductorError {
    Busy,
    SchemaUnavailable(String),
    ExecutorFailed(String),
}

#[derive(Debug, Clone)]
pub enum ExecutorError {
    DspFailed(String),
    BlobStoreFailed(String),
}

/// Parameters from HTTP handler → Conductor (R2)
#[derive(Debug, Clone)]
pub struct MasteringParams {
    pub audio_path: String,
    pub preset_id: String,
    pub target_lufs: f32,
    pub max_tp_db: f32,
    pub session_id: String,
    pub project_id: Option<String>,
    pub track_id: Option<String>,
    pub flavour_id: Option<String>,
    /// Per-track creative knobs. None for every current caller —
    /// no UI path sends these for album batch tracks today. Added
    /// so the v3 dispatch (Part 2b-ii) doesn't silently discard
    /// something that should have existed alongside flavour_id
    /// from the start; this was a real gap, not a deliberate
    /// omission, per recon 2026-07-18.
    pub intent_tone: Option<f32>,
    pub intent_dynamics: Option<f32>,
    pub chaos_seed: Option<u64>,
}

/// Plan from Conductor (R2) → Executor (R3)
/// Pure data. No logic inside.
/// Executor runs DspAdapter::master() from this — no decisions.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub audio_path: String,
    pub preset_id: String,
    pub target_lufs: f32,
    pub max_tp_db: f32,
    pub session_id: String,
}

/// Output from Executor (R3) → Conductor (R2)
#[derive(Debug, Clone)]
pub struct DspOutput {
    pub blob_id: String,
    pub lufs: f32,
    pub true_peak: f32,
    pub pcm_data: Option<std::path::PathBuf>,
    pub num_frames: usize,
    pub sample_rate: u32,
}

/// Analysis result from Executor pre-pass (decode + PreAnalyzer only)
/// No DSP — pure measurement. Used for album cohesion.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub session_id: String,
    pub integrated_lufs: f32,
    pub true_peak_dbtp: f32,
    pub bpm: f32,
}

/// Output from Conductor (R2) → HTTP handler
#[derive(Debug, Clone)]
pub struct MasteringOutput {
    pub job_id: String,
    pub blob_id: String,
    pub status: &'static str,
    pub pcm_data: Option<std::path::PathBuf>,
    pub num_frames: usize,
    pub sample_rate: u32,
}

#[derive(Debug, Clone)]
pub struct StreamingParams {
    pub audio_path: String,
    pub preset_id: String,
    pub flavour_id: Option<String>,
    pub intent_tone: Option<f32>,
    pub intent_dynamics: Option<f32>,
    /// Album cohesion override: when Some, this LUFS target wins
    /// over the preset's default (LoudnessTarget::from_preset).
    /// None for every normal single-track master today — album
    /// cohesion is the only intended caller, wired separately
    /// (Part 2b).
    pub target_lufs_override: Option<f32>,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct StreamingPlan {
    pub audio_path: String,
    pub preset_id: String,
    pub flavour_id: Option<String>,
    pub intent_tone: Option<f32>,
    pub intent_dynamics: Option<f32>,
    /// Album cohesion override: when Some, this LUFS target wins
    /// over the preset's default (LoudnessTarget::from_preset).
    /// None for every normal single-track master today — album
    /// cohesion is the only intended caller, wired separately
    /// (Part 2b).
    pub target_lufs_override: Option<f32>,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct StreamingOutput {
    pub job_id: String,
    pub blob_id: String,
    pub status: &'static str,
    pub pcm_data: Option<std::path::PathBuf>,
    pub num_frames: usize,
    pub sample_rate: u32,
    /// Real content hash of the mastered output (from the C1
    /// measured wav→raw pass) — was previously unavailable to
    /// callers, forcing album cohesion to substitute session_id as
    /// a fake input_hash (found 2026-07-18).
    pub pcm_blake3: String,
    /// Actual POST-mastering integrated LUFS — was previously
    /// unavailable to callers, forcing album cohesion to report the
    /// PRE-mastering measurement instead (found 2026-07-18).
    pub output_lufs: f32,
}

/// Output for a single track in a batch job
#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchTrackOutput {
    pub track_index: usize,
    pub session_id: String,
    pub blob_id: String,
    pub status: &'static str, // "ok" | "error"
    pub error: Option<String>,
    pub pcm_blake3: String,
    pub output_lufs: f32,
}

/// One finding from WizardAgent (R2)
#[derive(Debug, Clone, serde::Serialize)]
pub struct Finding {
    pub category: String,
    pub severity: String,
    pub message: String,
}

/// Metrics passed to WizardAgent (R2)
#[derive(Debug, Clone)]
pub struct AudioMetrics {
    pub integrated_lufs: f32,
    pub true_peak_dbtp: f32,
    pub lra_lu: f32,
}

/// The Operator owns all agent channels.
/// HTTP handlers hold a clone and call dispatch().
#[derive(Clone)]
pub struct Operator {
    schema_tx: mpsc::Sender<Intent>,
    conductor_tx: mpsc::Sender<Intent>,
    executor_tx: mpsc::Sender<Intent>,
    wizard_tx: mpsc::Sender<Intent>,
    audit: Arc<AuditLog>,
}

impl Operator {
    pub fn new(
        schema_tx: mpsc::Sender<Intent>,
        conductor_tx: mpsc::Sender<Intent>,
        executor_tx: mpsc::Sender<Intent>,
        wizard_tx: mpsc::Sender<Intent>,
        audit: Arc<AuditLog>,
    ) -> Self {
        Self {
            schema_tx,
            conductor_tx,
            executor_tx,
            wizard_tx,
            audit,
        }
    }

    pub async fn dispatch(&self, intent: Intent) -> Result<(), String> {
        match &intent {
            Intent::ValidateSchema { .. } | Intent::QuerySchema { .. } => {
                self.audit
                    .write(AuditEntry::new(
                        "operator.dispatch",
                        AuditLevel::Audit,
                        "→ SchemaAgent",
                    ))
                    .ok();
                self.schema_tx
                    .send(intent)
                    .await
                    .map_err(|e| format!("SchemaAgent closed: {e}"))
            }
            Intent::ExecuteBatchMastering { batch_id, .. } => {
                self.audit
                    .write(AuditEntry::new(
                        "operator.dispatch",
                        AuditLevel::Audit,
                        &format!("→ Conductor batch={batch_id}"),
                    ))
                    .ok();
                self.conductor_tx
                    .send(intent)
                    .await
                    .map_err(|e| format!("Conductor closed: {e}"))
            }
            Intent::ExecuteMastering { params, .. } => {
                self.audit
                    .write(AuditEntry::new(
                        "operator.dispatch",
                        AuditLevel::Audit,
                        &format!("→ Conductor session={}", params.session_id),
                    ))
                    .ok();
                self.conductor_tx
                    .send(intent)
                    .await
                    .map_err(|e| format!("Conductor closed: {e}"))
            }
            Intent::ExecuteStreaming { params, .. } => {
                self.audit
                    .write(AuditEntry::new(
                        "operator.dispatch",
                        AuditLevel::Audit,
                        &format!("→ Conductor streaming session={}", params.session_id),
                    ))
                    .ok();
                self.conductor_tx
                    .send(intent)
                    .await
                    .map_err(|e| format!("Conductor closed: {e}"))
            }
            Intent::RunDsp { .. } => {
                self.audit
                    .write(AuditEntry::new(
                        "operator.dispatch",
                        AuditLevel::Audit,
                        "→ Executor",
                    ))
                    .ok();
                self.executor_tx
                    .send(intent)
                    .await
                    .map_err(|e| format!("Executor closed: {e}"))
            }
            Intent::RunStreaming { .. } => {
                self.audit
                    .write(AuditEntry::new(
                        "operator.dispatch",
                        AuditLevel::Audit,
                        "→ Executor (streaming)",
                    ))
                    .ok();
                self.executor_tx
                    .send(intent)
                    .await
                    .map_err(|e| format!("Executor closed: {e}"))
            }
            Intent::RunAnalysis { .. } => {
                self.audit
                    .write(AuditEntry::new(
                        "operator.dispatch",
                        AuditLevel::Audit,
                        "→ Executor (analysis only)",
                    ))
                    .ok();
                self.executor_tx
                    .send(intent)
                    .await
                    .map_err(|e| format!("Executor closed: {e}"))
            }
            Intent::AnalyzeTelemetry { .. } => {
                self.audit
                    .write(AuditEntry::new(
                        "operator.dispatch",
                        AuditLevel::Audit,
                        "→ WizardAgent",
                    ))
                    .ok();
                self.wizard_tx
                    .send(intent)
                    .await
                    .map_err(|e| format!("WizardAgent closed: {e}"))
            }
            Intent::Shutdown => {
                self.audit
                    .write(AuditEntry::new(
                        "operator.dispatch",
                        AuditLevel::Audit,
                        "→ ALL (Shutdown)",
                    ))
                    .ok();
                let _ = self.schema_tx.send(Intent::Shutdown).await;
                let _ = self.conductor_tx.send(Intent::Shutdown).await;
                let _ = self.executor_tx.send(Intent::Shutdown).await;
                let _ = self.wizard_tx.send(Intent::Shutdown).await;
                Ok(())
            }
        }
    }
}

pub struct AgentHandles {
    pub schema: tokio::task::JoinHandle<()>,
    pub conductor: tokio::task::JoinHandle<()>,
    pub executor: tokio::task::JoinHandle<()>,
    pub wizard: tokio::task::JoinHandle<()>,
}

/// Spawn all four agent tasks. Call once from main().
/// Returns (Operator, AgentHandles) — wire into AppState and Graceful Shutdown.
// allow: 8 args; a params-struct refactor is deliberately deferred — not done as a clippy side-fix
#[allow(clippy::too_many_arguments)]
pub fn spawn_agents(
    audit: Arc<AuditLog>,
    head_state_ptr: Arc<ArcSwap<DspState>>,
    db: crate::db::DbConn,
    blob_store: crate::blob_store::BlobStore,
    album_tx: tokio::sync::broadcast::Sender<crate::app_state::AlbumEvent>,
    progress_tx: tokio::sync::broadcast::Sender<crate::app_state::MasteringProgress>,
    progress_map: Arc<dashmap::DashMap<String, crate::app_state::MasteringProgress>>,
    config: Arc<crate::config::M0Config>,
) -> (Operator, AgentHandles) {
    let (schema_tx, schema_rx) = mpsc::channel::<Intent>(32);
    let (conductor_tx, conductor_rx) = mpsc::channel::<Intent>(32);
    let (executor_tx, executor_rx) = mpsc::channel::<Intent>(32);
    let (wizard_tx, wizard_rx) = mpsc::channel::<Intent>(32);

    let schema = tokio::spawn(crate::agents::schema::run(schema_rx));
    let conductor = tokio::spawn(crate::agents::conductor::run(
        conductor_rx,
        head_state_ptr.clone(),
        db.clone(),
        blob_store.clone(),
        album_tx,
        progress_tx.clone(),
        progress_map.clone(),
        config.clone(),
    ));
    let executor = tokio::spawn(crate::agents::executor::run(
        executor_rx,
        head_state_ptr,
        db.clone(),
        blob_store,
        progress_tx,
        progress_map,
        config.state_path.clone(),
    ));
    let wizard = tokio::spawn(crate::agents::wizard::run(wizard_rx));

    let operator = Operator::new(schema_tx, conductor_tx, executor_tx, wizard_tx, audit);
    let handles = AgentHandles {
        schema,
        conductor,
        executor,
        wizard,
    };

    (operator, handles)
}
