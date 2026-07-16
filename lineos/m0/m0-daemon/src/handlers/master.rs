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

/// Batch/Album mastering request.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchMasterRequest {
    pub tracks: Vec<MasterRequest>,
    pub preset_id: Option<String>,
    pub album_flavour: Option<String>,
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

/// POST /master/batch — submit an album for sequential mastering.
/// Returns batch_id immediately. Progress via GET /progress/:batch_id
pub async fn trigger_batch_mastering(
    State(state): State<AppState>,
    Json(req): Json<BatchMasterRequest>,
) -> Json<serde_json::Value> {
    use crate::agents::operator::{Intent, MasteringParams};
    use tokio::sync::oneshot;

    if req.tracks.is_empty() {
        return Json(serde_json::json!({
            "error": "tracks array is empty"
        }));
    }

    let batch_id = uuid::Uuid::new_v4().to_string();
    let total = req.tracks.len();

    // Register all tracks as QUEUED immediately
    for (i, _track) in req.tracks.iter().enumerate() {
        let track_key = format!("{}:{}", batch_id, i);
        state.progress.insert(
            track_key,
            crate::app_state::MasteringProgress {
                job_id: format!("{}:{}", batch_id, i),
                stage: "QUEUED".into(),
                elapsed_ms: 0,
                blob_id: None,
                error: None,
            },
        );
    }

    // Register batch summary entry
    state.progress.insert(
        batch_id.clone(),
        crate::app_state::MasteringProgress {
            job_id: batch_id.clone(),
            stage: "BATCH_STARTED".into(),
            elapsed_ms: 0,
            blob_id: None,
            error: None,
        },
    );

    state
        .audit
        .write(crate::audit::AuditEntry::new(
            "m0d.batch_mastering_dispatched",
            crate::audit::AuditLevel::Audit,
            &format!("batch={} tracks={}", batch_id, total),
        ))
        .ok();

    // Build Vec<MasteringParams> from tracks
    let preset_override = req.preset_id.clone();
    let album_flavour = req.album_flavour.clone();
    let items: Vec<MasteringParams> = req
        .tracks
        .into_iter()
        .enumerate()
        .map(|(i, track)| MasteringParams {
            audio_path: track.audio_path,
            preset_id: preset_override.clone().unwrap_or(track.preset_id),
            target_lufs: -14.0_f32,
            max_tp_db: -1.0_f32,
            session_id: format!("{}:{}", batch_id, i),
            project_id: track.project_id,
            track_id: track.track_id,
            // Album flavour overrides per-track flavour if set
            // Engineer's sonic vision for the whole album
            flavour_id: album_flavour.clone().or(track.flavour_id),
            chaos_seed: track.chaos_seed,
        })
        .collect();

    // Dispatch to Conductor — non-blocking
    let (tx, rx) = oneshot::channel();
    let intent = Intent::ExecuteBatchMastering {
        batch_id: batch_id.clone(),
        items,
        response: tx,
    };

    let state_bg = state.clone();
    let batch_bg = batch_id.clone();

    tokio::spawn(async move {
        if state_bg.operator.dispatch(intent).await.is_err() {
            state_bg.progress.insert(
                batch_bg.clone(),
                crate::app_state::MasteringProgress {
                    job_id: batch_bg,
                    stage: "ERROR".into(),
                    elapsed_ms: 0,
                    blob_id: None,
                    error: Some("Conductor channel closed".into()),
                },
            );
            return;
        }

        match rx.await {
            Ok(Ok(outputs)) => {
                let ok_count = outputs.iter().filter(|o| o.status == "ok").count();

                // Update individual track progress
                for output in &outputs {
                    state_bg.progress.insert(
                        output.session_id.clone(),
                        crate::app_state::MasteringProgress {
                            job_id: output.session_id.clone(),
                            stage: if output.status == "ok" {
                                "CERTIFIED".into()
                            } else {
                                "ERROR".into()
                            },
                            elapsed_ms: 0,
                            blob_id: if output.blob_id.is_empty() {
                                None
                            } else {
                                Some(output.blob_id.clone())
                            },
                            error: output.error.clone(),
                        },
                    );
                }

                // Update batch summary
                state_bg.progress.insert(
                    batch_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id: batch_bg,
                        stage: format!("BATCH_COMPLETE {}/{}", ok_count, outputs.len()),
                        elapsed_ms: 0,
                        blob_id: None,
                        error: None,
                    },
                );
            }
            Ok(Err(e)) => {
                state_bg.progress.insert(
                    batch_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id: batch_bg,
                        stage: "ERROR".into(),
                        elapsed_ms: 0,
                        blob_id: None,
                        error: Some(format!("{:?}", e)),
                    },
                );
            }
            Err(_) => {
                state_bg.progress.insert(
                    batch_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id: batch_bg,
                        stage: "ERROR".into(),
                        elapsed_ms: 0,
                        blob_id: None,
                        error: Some("Conductor dropped batch response".into()),
                    },
                );
            }
        }
    });

    Json(serde_json::json!({
        "batch_id": batch_id,
        "tracks":   total,
        "status":   "queued"
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingRequest {
    pub audio_path: String,
    pub output_path: String,
    pub preset_id: String,
    pub flavour_id: Option<String>,
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
        output_path: req.output_path,
        preset_id: req.preset_id.clone(),
        flavour_id: req.flavour_id.clone(),
        session_id: session_id.clone(),
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
            Ok(Ok(output)) => {
                let blob_id_str = output.blob_id.clone();
                state_bg.progress.insert(
                    session_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id: session_bg,
                        stage: "COMPLETED".into(),
                        elapsed_ms: 0,
                        blob_id: Some(blob_id_str.clone()),
                        error: None,
                    },
                );

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

    // Return job_id IMMEDIATELY
    Json(serde_json::json!({ "job_id": session_id }))
}
