//! Executor — R3: executes DSP plan. Pure action, no decisions.
//! Receives ExecutionPlan from Conductor. Calls run_dsp_internal().
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.3
//! Motto: "I do not think. I do."
//!
//! run_dsp_internal() is the entire DSP pipeline — decode, NMF,
//! spatial, mastering, certification. Executor wraps it in
//! spawn_blocking and returns DspOutput to Conductor.

use super::operator::{DspOutput, ExecutorError, Intent};
use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::mpsc;
use xaak::repo::DspState;

pub async fn run(
    mut rx: mpsc::Receiver<Intent>,
    head_state_ptr: Arc<ArcSwap<DspState>>,
    db: crate::db::DbConn,
    blob_store: crate::blob_store::BlobStore,
    progress_tx: tokio::sync::broadcast::Sender<crate::app_state::MasteringProgress>,
    progress_map: Arc<dashmap::DashMap<String, crate::app_state::MasteringProgress>>,
) {
    while let Some(intent) = rx.recv().await {
        match intent {
            Intent::Shutdown => break,

            Intent::RunDsp { plan, response } => {
                // R3: pure extraction + execution.
                // No if/else on parameter values.
                // No decisions — only extraction from plan.
                let audio_path = plan.audio_path.clone();
                let preset_id = plan.preset_id.clone();
                let _target_lufs = plan.target_lufs;
                let _max_tp_db = plan.max_tp_db;

                // Build MasterRequest from ExecutionPlan — pure extraction
                let req = crate::handlers::master::MasterRequest {
                    audio_path,
                    preset_id,
                    flavour_id: None,
                    intent_tone: None,
                    intent_dynamics: None,
                    persona_id: None,
                    tone: None,
                    dynamics: None,
                    chaos_seed: None,
                    project_id: None,
                    track_id: None,
                    mix_levels: None,
                    preview_id: None,
                };

                let start = std::time::Instant::now();

                // spawn_blocking: DSP is CPU-intensive, must not block async runtime
                let head_state = head_state_ptr.clone();
                let p_tx = progress_tx.clone();
                let p_map = progress_map.clone();
                let j_id = plan.session_id.clone();
                let result = tokio::task::spawn_blocking(move || {
                    crate::domain::dsp_pipeline::run_dsp(
                        &req,
                        start,
                        head_state,
                        Some(p_tx),
                        Some(p_map),
                        j_id,
                    )
                })
                .await;

                match result {
                    Err(e) => {
                        let _ = response.send(Err(ExecutorError::DspFailed(format!(
                            "spawn_blocking join error: {e}"
                        ))));
                    }
                    Ok(Err(e)) => {
                        let _ = response.send(Err(ExecutorError::DspFailed(e)));
                    }
                    Ok(Ok((blob, mastered_path, user_model_opt))) => {
                        // Executor: persist UserMarkovModel to ~/.creator_os/state/
                        // Zero file I/O in DSP layer — this is the correct layer
                        // Executor: persist UserMarkovModel — single overwrite
                        if let Some(ref model) = user_model_opt {
                            let state_dir = format!(
                                "{}/.creator_os/state",
                                std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
                            );
                            let _ = std::fs::create_dir_all(&state_dir);
                            if let Ok(json) = model.to_json() {
                                let model_path = format!("{}/user_model_corpus.json", state_dir);
                                let _ = std::fs::write(&model_path, json);
                            }
                        }

                        // Persist Track to SurrealDB
                        let track_lufs = blob.loudness.integrated_lufs;
                        let track_tp = blob.loudness.true_peak_dbtp;
                        let track_blob = blob.id.clone();
                        let track_path = blob.audio_path.to_string_lossy().to_string();
                        let db_clone = db.clone();
                        tokio::spawn(async move {
                            let created_at = chrono::Utc::now().to_rfc3339();
                            let aql = format!(
                                "CREATE tracks CONTENT {{ \
                                    blob_id: '{}', \
                                    audio_path: '{}', \
                                    lufs: {}, \
                                    true_peak: {}, \
                                    created_at: '{}', \
                                    project_id: 'default', \
                                    flavour_id: 'neutral', \
                                    duration_ms: 0 \
                                }}",
                                track_blob.replace('\'', "\\'"),
                                track_path.replace('\'', "\\'"),
                                track_lufs,
                                track_tp,
                                created_at,
                            );
                            let _ = db_clone.query(aql).await;
                        });

                        blob_store.insert(blob.clone());

                        let p = crate::app_state::MasteringProgress {
                            job_id: plan.session_id.clone(),
                            stage: "CERTIFIED".into(),
                            elapsed_ms: start.elapsed().as_millis() as u64,
                            blob_id: Some(blob.id.clone()),
                            error: None,
                        };
                        progress_map.insert(plan.session_id.clone(), p.clone());
                        let _ = progress_tx.send(p);

                        let output = DspOutput {
                            blob_id: blob.id.clone(),
                            lufs: blob.loudness.integrated_lufs,
                            true_peak: blob.loudness.true_peak_dbtp,
                            pcm_data: Some(mastered_path),
                            num_frames: blob.num_frames,
                            sample_rate: blob.sample_rate,
                        };
                        let _ = response.send(Ok(output));
                    }
                }
            }

            Intent::RunAnalysis {
                audio_path,
                session_id,
                response,
            } => {
                // R3: decode + PreAnalyzer only. No DSP. No decisions.
                let result = tokio::task::spawn_blocking(move || {
                    let payload =
                        crate::handlers::decode::decode_smart(&audio_path).map_err(|e| {
                            super::operator::ExecutorError::DspFailed(format!("Decode error: {e}"))
                        })?;
                    let stereo = payload.to_stereo_for_telemetry();
                    use sp314_dsp::analysis::PreAnalyzer;
                    let analysis =
                        PreAnalyzer::run(&stereo.left, &stereo.right, stereo.sample_rate);

                    // TODO: Auto input-trim based on pre-analysis loudness.
                    // Replaces manual INPUT TRIM removed from UI.
                    // Calculate headroom and apply gain before DSP chain.

                    Ok(super::operator::AnalysisResult {
                        session_id,
                        integrated_lufs: analysis.integrated_lufs,
                        true_peak_dbtp: analysis.true_peak_dbtp,
                        bpm: analysis.bpm,
                    })
                })
                .await
                .map_err(|e| {
                    super::operator::ExecutorError::DspFailed(format!("spawn_blocking join: {e}"))
                })
                .and_then(|r| r);
                let _ = response.send(result);
            }

            _ => {}
        }
    }
}
