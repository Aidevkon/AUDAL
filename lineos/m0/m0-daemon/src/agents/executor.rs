//! Executor — R3: executes DSP plan. Pure action, no decisions.
//! Receives ExecutionPlan from Conductor. Calls run_dsp_internal().
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.3
//! Motto: "I do not think. I do."
//!
//! run_dsp_internal() is the entire DSP pipeline — decode, NMF,
//! spatial, mastering, certification. Executor wraps it in
//! spawn_blocking and returns DspOutput to Conductor.

use tokio::sync::mpsc;
use super::operator::{Intent, ExecutorError, DspOutput};

pub async fn run(mut rx: mpsc::Receiver<Intent>) {
    while let Some(intent) = rx.recv().await {
        match intent {
            Intent::Shutdown => break,

            Intent::RunDsp { plan, response } => {
                // R3: pure extraction + execution.
                // No if/else on parameter values.
                // No decisions — only extraction from plan.
                let audio_path  = plan.audio_path.clone();
                let preset_id   = plan.preset_id.clone();
                let _target_lufs = plan.target_lufs;
                let _max_tp_db   = plan.max_tp_db;

                // Build MasterRequest from ExecutionPlan — pure extraction
                let req = crate::handlers::master::MasterRequest {
                    audio_path,
                    preset_id,
                    flavour_id:      None,
                    intent_tone:     None,
                    intent_dynamics: None,
                    persona_id:      None,
                    tone:            None,
                    dynamics:        None,
                    chaos_seed:      None,
                    project_id:      None,
                    track_id:        None,
                    mix_levels:      None,
                    preview_id:      None,
                };

                let start = std::time::Instant::now();

                // spawn_blocking: DSP is CPU-intensive, must not block async runtime
                let result = tokio::task::spawn_blocking(move || {
                    tokio::runtime::Handle::current().block_on(async {
                        crate::handlers::master::run_dsp(
                            &req,
                            start,
                        ).await
                    })
                }).await;

                match result {
                    Err(e) => {
                        let _ = response.send(Err(
                            ExecutorError::DspFailed(
                                format!("spawn_blocking join error: {e}")
                            )
                        ));
                    }
                    Ok(Err(e)) => {
                        let _ = response.send(Err(
                            ExecutorError::DspFailed(e)
                        ));
                    }
                    Ok(Ok((blob, _chunk_original, _target_lufs))) => {
                        let output = DspOutput {
                            blob_id:   blob.id.clone(),
                            lufs:      blob.loudness.integrated_lufs,
                            true_peak: blob.loudness.true_peak_dbtp,
                            pcm_data:  Some(_chunk_original),
                        };
                        // Store blob — Executor is responsible for persistence
                        // This is execution, not decision-making
                        let _ = response.send(Ok(output));
                    }
                }
            }

            Intent::RunAnalysis { audio_path, session_id, response } => {
                // R3: decode + PreAnalyzer only. No DSP. No decisions.
                let result = tokio::task::spawn_blocking(move || {
                    let pcm = crate::handlers::decode::decode_audio(&audio_path)
                        .map_err(|e| super::operator::ExecutorError::DspFailed(
                            format!("Decode error: {e}")
                        ))?;
                    let left:  Vec<f32> = pcm.samples.iter()
                        .step_by(2).copied().collect();
                    let right: Vec<f32> = pcm.samples.iter()
                        .skip(1).step_by(2).copied().collect();
                    use sp314_dsp::analysis::PreAnalyzer;
                    let analysis = PreAnalyzer::run(&left, &right, pcm.sample_rate);
                    Ok(super::operator::AnalysisResult {
                        session_id,
                        integrated_lufs: analysis.integrated_lufs,
                        true_peak_dbtp:  analysis.true_peak_dbtp,
                    })
                }).await
                    .map_err(|e| super::operator::ExecutorError::DspFailed(
                        format!("spawn_blocking join: {e}")
                    ))
                    .and_then(|r| r);
                let _ = response.send(result);
            }

            _ => {}
        }
    }
}
