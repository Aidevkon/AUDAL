//! Operator — Intent enum + channel ownership + dispatch routing.
//! Authority: Constitutional Agent Architecture Spec v3.1
//!
//! Single entry point for all cross-agent communication.
//! Every Intent is logged before dispatch.
//! No agent calls another agent directly.

use tokio::sync::{mpsc, oneshot};
use crate::audit::{AuditLog, AuditEntry, AuditLevel};
use std::sync::Arc;

#[derive(Debug)]
pub enum Intent {
    // R1 — Schema
    ValidateSchema {
        patch:    serde_json::Value,
        response: oneshot::Sender<Result<(), SchemaError>>,
    },
    QuerySchema {
        path:     String,
        response: oneshot::Sender<serde_json::Value>,
    },
    // R2 — Conductor → full mastering workflow
    ExecuteMastering {
        params:   MasteringParams,
        response: oneshot::Sender<Result<MasteringOutput, ConductorError>>,
    },
    // R3 — Conductor → Executor
    RunDsp {
        plan:     ExecutionPlan,
        response: oneshot::Sender<Result<DspOutput, ExecutorError>>,
    },
    // R3 — analysis only (decode + PreAnalyzer, no DSP)
    RunAnalysis {
        audio_path: String,
        session_id: String,
        response:   oneshot::Sender<Result<AnalysisResult, ExecutorError>>,
    },
    // R2 — Conductor batch workflow
    ExecuteBatchMastering {
        batch_id: String,
        items:    Vec<MasteringParams>,
        response: oneshot::Sender<Result<Vec<BatchTrackOutput>, ConductorError>>,
    },
    // R2 — WizardAgent
    AnalyzeTelemetry {
        metrics:  AudioMetrics,
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
    pub audio_path:   String,
    pub preset_id:    String,
    pub target_lufs:  f32,
    pub max_tp_db:    f32,
    pub session_id:   String,
    pub project_id:   Option<String>,
    pub track_id:     Option<String>,
    pub flavour_id:   Option<String>,
    pub chaos_seed:   Option<u64>,
}

/// Plan from Conductor (R2) → Executor (R3)
/// Pure data. No logic inside.
/// Executor runs DspAdapter::master() from this — no decisions.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub audio_path:  String,
    pub preset_id:   String,
    pub target_lufs: f32,
    pub max_tp_db:   f32,
    pub session_id:  String,
}

/// Output from Executor (R3) → Conductor (R2)
#[derive(Debug, Clone)]
pub struct DspOutput {
    pub blob_id:    String,
    pub lufs:       f32,
    pub true_peak:  f32,
    pub pcm_data:   Option<std::path::PathBuf>,
    pub num_frames: usize,
}

/// Analysis result from Executor pre-pass (decode + PreAnalyzer only)
/// No DSP — pure measurement. Used for album cohesion.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub session_id:      String,
    pub integrated_lufs: f32,
    pub true_peak_dbtp:  f32,
}

/// Output from Conductor (R2) → HTTP handler
#[derive(Debug, Clone)]
pub struct MasteringOutput {
    pub job_id:     String,
    pub blob_id:    String,
    pub status:     &'static str,
    pub pcm_data:   Option<std::path::PathBuf>,
    pub num_frames: usize,
}

/// Output for a single track in a batch job
#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchTrackOutput {
    pub track_index: usize,
    pub session_id:  String,
    pub blob_id:     String,
    pub status:      &'static str,  // "ok" | "error"
    pub error:       Option<String>,
}

/// One finding from WizardAgent (R2)
#[derive(Debug, Clone, serde::Serialize)]
pub struct Finding {
    pub category: String,
    pub severity: String,
    pub message:  String,
}

/// Metrics passed to WizardAgent (R2)
#[derive(Debug, Clone)]
pub struct AudioMetrics {
    pub integrated_lufs: f32,
    pub true_peak_dbtp:  f32,
    pub lra_lu:          f32,
}

/// The Operator owns all agent channels.
/// HTTP handlers hold a clone and call dispatch().
#[derive(Clone)]
pub struct Operator {
    schema_tx:    mpsc::Sender<Intent>,
    conductor_tx: mpsc::Sender<Intent>,
    executor_tx:  mpsc::Sender<Intent>,
    wizard_tx:    mpsc::Sender<Intent>,
    audit:        Arc<AuditLog>,
}

impl Operator {
    pub fn new(
        schema_tx:    mpsc::Sender<Intent>,
        conductor_tx: mpsc::Sender<Intent>,
        executor_tx:  mpsc::Sender<Intent>,
        wizard_tx:    mpsc::Sender<Intent>,
        audit:        Arc<AuditLog>,
    ) -> Self {
        Self { schema_tx, conductor_tx, executor_tx, wizard_tx, audit }
    }

    pub async fn dispatch(&self, intent: Intent) -> Result<(), String> {
        match &intent {
            Intent::ValidateSchema { .. } | Intent::QuerySchema { .. } => {
                self.audit.write(AuditEntry::new(
                    "operator.dispatch", AuditLevel::Audit, "→ SchemaAgent",
                )).ok();
                self.schema_tx.send(intent).await
                    .map_err(|e| format!("SchemaAgent closed: {e}"))
            }
            Intent::ExecuteBatchMastering { batch_id, .. } => {
                self.audit.write(AuditEntry::new(
                    "operator.dispatch", AuditLevel::Audit,
                    &format!("→ Conductor batch={batch_id}"),
                )).ok();
                self.conductor_tx.send(intent).await
                    .map_err(|e| format!("Conductor closed: {e}"))
            }
            Intent::ExecuteMastering { params, .. } => {
                self.audit.write(AuditEntry::new(
                    "operator.dispatch", AuditLevel::Audit,
                    &format!("→ Conductor session={}", params.session_id),
                )).ok();
                self.conductor_tx.send(intent).await
                    .map_err(|e| format!("Conductor closed: {e}"))
            }
            Intent::RunDsp { .. } => {
                self.audit.write(AuditEntry::new(
                    "operator.dispatch", AuditLevel::Audit, "→ Executor",
                )).ok();
                self.executor_tx.send(intent).await
                    .map_err(|e| format!("Executor closed: {e}"))
            }
            Intent::RunAnalysis { .. } => {
                self.audit.write(AuditEntry::new(
                    "operator.dispatch", AuditLevel::Audit,
                    "→ Executor (analysis only)",
                )).ok();
                self.executor_tx.send(intent).await
                    .map_err(|e| format!("Executor closed: {e}"))
            }
            Intent::AnalyzeTelemetry { .. } => {
                self.audit.write(AuditEntry::new(
                    "operator.dispatch", AuditLevel::Audit, "→ WizardAgent",
                )).ok();
                self.wizard_tx.send(intent).await
                    .map_err(|e| format!("WizardAgent closed: {e}"))
            }
            Intent::Shutdown => {
                self.audit.write(AuditEntry::new(
                    "operator.dispatch", AuditLevel::Audit, "→ ALL (Shutdown)",
                )).ok();
                let _ = self.schema_tx.send(Intent::Shutdown).await;
                let _ = self.conductor_tx.send(Intent::Shutdown).await;
                let _ = self.executor_tx.send(Intent::Shutdown).await;
                let _ = self.wizard_tx.send(Intent::Shutdown).await;
                Ok(())
            }
        }
    }
}

/// Spawn all four agent tasks. Call once from main().
/// Returns Operator — wire into AppState.
pub fn spawn_agents(audit: Arc<AuditLog>) -> Operator {
    let (schema_tx,    schema_rx)    = mpsc::channel::<Intent>(32);
    let (conductor_tx, conductor_rx) = mpsc::channel::<Intent>(32);
    let (executor_tx,  executor_rx)  = mpsc::channel::<Intent>(32);
    let (wizard_tx,    wizard_rx)    = mpsc::channel::<Intent>(32);

    tokio::spawn(crate::agents::schema::run(schema_rx));
    tokio::spawn(crate::agents::conductor::run(conductor_rx));
    tokio::spawn(crate::agents::executor::run(executor_rx));
    tokio::spawn(crate::agents::wizard::run(wizard_rx));

    Operator::new(schema_tx, conductor_tx, executor_tx, wizard_tx, audit)
}
