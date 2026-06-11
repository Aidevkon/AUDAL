//! Executor — R3: executes DSP plan. Pure action, no decisions.
//! Receives ExecutionPlan from Conductor. Calls run_dsp_internal().
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.3
//! Motto: "I do not think. I do."
//!
//! run_dsp_internal() is the entire DSP pipeline — decode, NMF,
//! spatial, mastering, certification. Executor wraps it in
//! spawn_blocking and returns DspOutput to Conductor.

use super::operator::{DspOutput, ExecutorError, Intent};
use tokio::sync::mpsc;
use std::sync::Arc;
use arc_swap::ArcSwap;
use xaak::repo::DspState;

pub async fn run(mut rx: mpsc::Receiver<Intent>, head_state_ptr: Arc<ArcSwap<DspState>>) {
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
                let result = tokio::task::spawn_blocking(move || {
                    crate::domain::dsp_pipeline::run_dsp(&req, start, head_state)
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
                    Ok(Ok((blob, _chunk_original, user_model_opt))) => {
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
                                let model_path = format!(
                                    "{}/user_model_corpus.json", state_dir
                                );
                                let _ = std::fs::write(&model_path, json);
                            }
                        }

                        let output = DspOutput {
                            blob_id: blob.id.clone(),
                            lufs:    blob.loudness.integrated_lufs,
                            true_peak: blob.loudness.true_peak_dbtp,
                            pcm_data: Some(_chunk_original),
                            num_frames: blob.num_frames,
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
                    let pcm = crate::handlers::decode::decode_audio(&audio_path).map_err(|e| {
                        super::operator::ExecutorError::DspFailed(format!("Decode error: {e}"))
                    })?;
                    let left: Vec<f32> = pcm.samples.iter().step_by(2).copied().collect();
                    let right: Vec<f32> = pcm.samples.iter().skip(1).step_by(2).copied().collect();
                    use sp314_dsp::analysis::PreAnalyzer;
                    let analysis = PreAnalyzer::run(&left, &right, pcm.sample_rate);
                    Ok(super::operator::AnalysisResult {
                        session_id,
                        integrated_lufs: analysis.integrated_lufs,
                        true_peak_dbtp: analysis.true_peak_dbtp,
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
