//! POST /master — trigger mastering pipeline.
//! Authority: Phase 6 task-decomposition P6-003
//! M0 Constitution v2.0 §03: M0 is the trust boundary for all DSP invocations.
//!
//! Invokes sp314-dsp MasteringPipeline::master() natively.
//! Stores resulting blob metrics in BlobStore. Returns blob_id to caller.
//!
//! FORBIDDEN: Returning serde_json::Value.
//! FORBIDDEN: Calling sp314-dsp from the Tauri backend directly.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;

// Phase 12A (A-003 §1): PCM ownership transfer to xaak after mastering

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterRequest {
    pub audio_path: String,
    pub preset_id: String,
    pub flavour_id: Option<String>,
    pub intent_tone: Option<f32>,
    pub intent_dynamics: Option<f32>,
    pub persona_id: Option<String>,
    pub tone: Option<f32>,
    pub dynamics: Option<f32>,
    pub chaos_seed: Option<u64>,
    pub project_id: Option<String>,
    pub track_id: Option<String>,
    /// Phase 8b: per-stem mix levels from 5.1 Spatial Mixer widget.
    /// None = default (all 1.0 — backward compatible).
    pub mix_levels: Option<MixLevels>,
    /// Phase 8a: preview session reference (future ScoutResult cache).
    pub preview_id: Option<String>,
}

/// Per-stem mix levels from 5.1 Spatial Mixer widget.
/// Applied before spatial rendering — INV-MX-1.
/// Default: all 1.0 (backward compatible, no change in behavior).
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MixLevels {
    pub voice: f32,
    pub drums: f32,
    pub bass: f32,
    pub harmonics: f32,
    pub ambience: f32,
}

impl Default for MixLevels {
    fn default() -> Self {
        Self {
            voice: 1.0,
            drums: 1.0,
            bass: 1.0,
            harmonics: 1.0,
            ambience: 1.0,
        }
    }
}

impl MixLevels {
    /// Clamp all levels to [0.0, 1.0] — constitutional safety.
    pub fn clamped(&self) -> Self {
        Self {
            voice: self.voice.clamp(0.0, 1.0),
            drums: self.drums.clamp(0.0, 1.0),
            bass: self.bass.clamp(0.0, 1.0),
            harmonics: self.harmonics.clamp(0.0, 1.0),
            ambience: self.ambience.clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MasterResponse {
    pub blob_id: String,
    pub status: &'static str, // "ok" | "error"
    pub message: Option<String>,
    pub aether_error: Option<String>,
    pub persona_id: Option<String>,
    pub converged: Option<bool>,
}

/// POST /master — run sp314-dsp → store blob → return blob_id.
pub async fn trigger_mastering(
    State(state): State<AppState>,
    Json(req): Json<MasterRequest>,
) -> Json<serde_json::Value> {
    use crate::agents::operator::{Intent, MasteringParams};
    use tokio::sync::oneshot;

    let session_id = uuid::Uuid::new_v4().to_string();

    let params = MasteringParams {
        audio_path: req.audio_path.clone(),
        preset_id: req.preset_id.clone(),
        target_lufs: -14.0_f32,
        max_tp_db: -1.0_f32,
        session_id: session_id.clone(),
        project_id: req.project_id.clone(),
        track_id: req.track_id.clone(),
        flavour_id: req.flavour_id.clone(),
        intent_tone: req.intent_tone,
        intent_dynamics: req.intent_dynamics,
        chaos_seed: req.chaos_seed,
    };

    // Register progress immediately
    state.progress.insert(
        session_id.clone(),
        crate::app_state::MasteringProgress {
            job_id: session_id.clone(),
            stage: "DISPATCHED".into(),
            elapsed_ms: 0,
            blob_id: None,
            error: None,
        },
    );

    state
        .audit
        .write(crate::audit::AuditEntry::new(
            "m0d.mastering_dispatched",
            crate::audit::AuditLevel::Audit,
            &format!(
                "session={} path={} preset={}",
                session_id, req.audio_path, req.preset_id
            ),
        ))
        .ok();

    // Dispatch to Conductor (R2) — non-blocking
    // HTTP handler does not wait for DSP completion
    let (tx, rx) = oneshot::channel();
    let intent = Intent::ExecuteMastering {
        params,
        response: tx,
    };

    let state_bg = state.clone();
    let session_bg = session_id.clone();

    tokio::spawn(async move {
        if state_bg.operator.dispatch(intent).await.is_err() {
            state_bg.progress.insert(
                session_bg.clone(),
                crate::app_state::MasteringProgress {
                    job_id: session_bg,
                    stage: "ERROR".into(),
                    elapsed_ms: 0,
                    blob_id: None,
                    error: Some("Conductor channel closed".into()),
                },
            );
            return;
        }

        match rx.await {
            Ok(Ok(mut output)) => {
                let blob_id_str = output.blob_id.clone();
                state_bg.progress.insert(
                    session_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id: session_bg,
                        stage: "CERTIFIED".into(),
                        elapsed_ms: 0,
                        blob_id: Some(blob_id_str.clone()),
                        error: None,
                    },
                );

                if let Some(path) = output.pcm_data.take() {
                    if let Ok(b_id) = uuid::Uuid::parse_str(&blob_id_str) {
                        let transfer = xaak::PcmTransfer {
                            pcm_path: path,
                            sample_rate: output.sample_rate,
                            channels: 2,
                            blob_id: b_id,
                            num_frames: output.num_frames,
                        };
                        state_bg.playback.load(transfer);

                        let raw_transfer = xaak::PcmTransfer {
                            pcm_path: std::path::PathBuf::from(format!(
                                "/tmp/m0d-raw-{}.pcm",
                                blob_id_str
                            )),
                            sample_rate: output.sample_rate,
                            channels: 2,
                            blob_id: b_id,
                            num_frames: output.num_frames,
                        };
                        state_bg.playback.load_raw(raw_transfer);
                    }
                }
            }
            Ok(Err(e)) => {
                state_bg.progress.insert(
                    session_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id: session_bg,
                        stage: "ERROR".into(),
                        elapsed_ms: 0,
                        blob_id: None,
                        error: Some(format!("{:?}", e)),
                    },
                );
            }
            Err(_) => {
                state_bg.progress.insert(
                    session_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id: session_bg,
                        stage: "ERROR".into(),
                        elapsed_ms: 0,
                        blob_id: None,
                        error: Some("Conductor dropped response".into()),
                    },
                );
            }
        }
    });

    // Return job_id IMMEDIATELY — HTTP does not wait for DSP
    Json(serde_json::json!({ "job_id": session_id }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingRequest {
    pub audio_path: String,
    pub preset_id: String,
    pub flavour_id: Option<String>,
    pub intent_tone: Option<f32>,
    pub intent_dynamics: Option<f32>,
    pub target_lufs_override: Option<f32>,
}

/// POST /master/streaming — run v3 streaming pipeline.
pub async fn trigger_streaming(
    State(state): State<AppState>,
    Json(req): Json<StreamingRequest>,
) -> Json<serde_json::Value> {
    use crate::agents::operator::{Intent, StreamingParams};
    use tokio::sync::oneshot;

    let session_id = uuid::Uuid::new_v4().to_string();

    let params = StreamingParams {
        audio_path: req.audio_path.clone(),
        preset_id: req.preset_id.clone(),
        flavour_id: req.flavour_id.clone(),
        intent_tone: req.intent_tone,
        intent_dynamics: req.intent_dynamics,
        target_lufs_override: req.target_lufs_override,
        session_id: session_id.clone(),
    };

    // Register progress immediately
    let progress = crate::app_state::MasteringProgress {
        job_id: session_id.clone(),
        stage: "DISPATCHED".into(),
        elapsed_ms: 0,
        blob_id: None,
        error: None,
    };
    state.progress.insert(session_id.clone(), progress.clone());
    let _ = state.progress_tx.send(progress);

    state
        .audit
        .write(crate::audit::AuditEntry::new(
            "m0d.streaming_dispatched",
            crate::audit::AuditLevel::Audit,
            &format!(
                "session={} path={} preset={}",
                session_id, req.audio_path, req.preset_id
            ),
        ))
        .ok();

    // Dispatch to Conductor — non-blocking
    let (tx, rx) = oneshot::channel();
    let intent = Intent::ExecuteStreaming {
        params,
        response: tx,
    };

    let state_bg = state.clone();
    let session_bg = session_id.clone();

    tokio::spawn(async move {
        if state_bg.operator.dispatch(intent).await.is_err() {
            let progress = crate::app_state::MasteringProgress {
                job_id: session_bg.clone(),
                stage: "ERROR".into(),
                elapsed_ms: 0,
                blob_id: None,
                error: Some("Conductor channel closed".into()),
            };
            state_bg
                .progress
                .insert(session_bg.clone(), progress.clone());
            let _ = state_bg.progress_tx.send(progress);
            return;
        }

        match rx.await {
            Ok(Ok(output)) => {
                let blob_id_str = output.blob_id.clone();
                let progress = crate::app_state::MasteringProgress {
                    job_id: session_bg.clone(),
                    stage: "CERTIFIED".into(), // certification now runs on the streamed output (Episode-parity streaming cert)
                    elapsed_ms: 0,
                    blob_id: Some(blob_id_str.clone()),
                    error: None,
                };
                state_bg
                    .progress
                    .insert(session_bg.clone(), progress.clone());
                let _ = state_bg.progress_tx.send(progress);

                if let Ok(b_id) = uuid::Uuid::parse_str(&blob_id_str) {
                    let transfer = xaak::PcmTransfer {
                        pcm_path: std::path::PathBuf::from(format!(
                            "/tmp/m0d-mastered-{}.pcm",
                            blob_id_str
                        )),
                        sample_rate: output.sample_rate,
                        channels: 2,
                        blob_id: b_id,
                        num_frames: output.num_frames,
                    };
                    state_bg.playback.load(transfer);

                    let raw_transfer = xaak::PcmTransfer {
                        pcm_path: std::path::PathBuf::from(format!(
                            "/tmp/m0d-raw-{}.pcm",
                            blob_id_str
                        )),
                        sample_rate: output.sample_rate,
                        channels: 2,
                        blob_id: b_id,
                        num_frames: output.num_frames,
                    };
                    state_bg.playback.load_raw(raw_transfer);
                }
            }
            Ok(Err(e)) => {
                let progress = crate::app_state::MasteringProgress {
                    job_id: session_bg.clone(),
                    stage: "ERROR".into(),
                    elapsed_ms: 0,
                    blob_id: None,
                    error: Some(format!("{:?}", e)),
                };
                state_bg
                    .progress
                    .insert(session_bg.clone(), progress.clone());
                let _ = state_bg.progress_tx.send(progress);
            }
            Err(_) => {
                let progress = crate::app_state::MasteringProgress {
                    job_id: session_bg.clone(),
                    stage: "ERROR".into(),
                    elapsed_ms: 0,
                    blob_id: None,
                    error: Some("Conductor dropped response".into()),
                };
                state_bg
                    .progress
                    .insert(session_bg.clone(), progress.clone());
                let _ = state_bg.progress_tx.send(progress);
            }
        }
    });

    // Return job_id IMMEDIATELY
    Json(serde_json::json!({ "job_id": session_id }))
}
