//! Conductor — R2: orchestrates mastering workflow.
//! Receives MasteringParams from HTTP handler.
//! Builds ExecutionPlan. Dispatches RunDsp to Executor.
//! Collects DspOutput. Returns MasteringOutput.
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.2
//! Motto: "I build the plan. I do not execute it."

use super::operator::{ConductorError, ExecutionPlan, ExecutorError, Intent, MasteringOutput};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use arc_swap::ArcSwap;
use xaak::repo::DspState;
use tokio::sync::{mpsc, oneshot};

pub async fn run(mut rx: mpsc::Receiver<Intent>, head_state_ptr: Arc<ArcSwap<DspState>>) {
    // AtomicBool: only one mastering job at a time
    // R2 decision: is the system busy?
    let busy = Arc::new(AtomicBool::new(false));

    // Conductor holds its own channel to Executor
    // Created once at startup — persists for the lifetime of the agent
    let (executor_tx, executor_rx) = mpsc::channel::<Intent>(4);
    tokio::spawn(super::executor::run(executor_rx, head_state_ptr));

    while let Some(intent) = rx.recv().await {
        match intent {
            Intent::Shutdown => break,

            Intent::ExecuteMastering { params, response } => {
                // R2 decision: busy check
                if busy.swap(true, Ordering::SeqCst) {
                    let _ = response.send(Err(ConductorError::Busy));
                    continue;
                }

                let executor_tx = executor_tx.clone();
                let busy_clone = busy.clone();

                // Spawn so HTTP handler is not blocked
                tokio::spawn(async move {
                    // R2: build ExecutionPlan from MasteringParams
                    // Pure translation — no business logic on values
                    let plan = ExecutionPlan {
                        audio_path: params.audio_path,
                        preset_id: params.preset_id,
                        target_lufs: params.target_lufs,
                        max_tp_db: params.max_tp_db,
                        session_id: params.session_id.clone(),
                    };

                    // Dispatch to Executor (R3)
                    let (tx, rx) = oneshot::channel();
                    let run_dsp_intent = Intent::RunDsp { plan, response: tx };

                    if executor_tx.send(run_dsp_intent).await.is_err() {
                        let _ = response.send(Err(ConductorError::ExecutorFailed(
                            "Executor channel closed".into(),
                        )));
                        busy_clone.store(false, Ordering::SeqCst);
                        return;
                    }

                    // Await Executor result
                    match rx.await {
                        Err(_) => {
                            let _ = response.send(Err(ConductorError::ExecutorFailed(
                                "Executor dropped oneshot".into(),
                            )));
                        }
                        Ok(Err(ExecutorError::DspFailed(e))) => {
                            let _ = response.send(Err(ConductorError::ExecutorFailed(e)));
                        }
                        Ok(Err(ExecutorError::BlobStoreFailed(e))) => {
                            let _ = response.send(Err(ConductorError::ExecutorFailed(e)));
                        }
                        Ok(Ok(dsp_output)) => {
                            // R2: assemble MasteringOutput from DspOutput
                            let output = MasteringOutput {
                                job_id: params.session_id,
                                blob_id: dsp_output.blob_id,
                                status: "ok",
                                pcm_data: dsp_output.pcm_data,
                                num_frames: dsp_output.num_frames,
                            };
                            let _ = response.send(Ok(output));
                        }
                    }

                    // Release busy flag
                    busy_clone.store(false, Ordering::SeqCst);
                });
            }

            Intent::ExecuteBatchMastering {
                batch_id,
                items,
                response,
            } => {
                // R2 decision: busy check
                if busy.swap(true, Ordering::SeqCst) {
                    let _ = response.send(Err(ConductorError::Busy));
                    continue;
                }

                let executor_tx = executor_tx.clone();
                let busy_clone = busy.clone();

                tokio::spawn(async move {
                    let total = items.len();
                    let mut outputs: Vec<super::operator::BatchTrackOutput> =
                        Vec::with_capacity(total);

                    // ── Album Cohesion Pre-Pass ──────────────────────────
                    // Step 1: analyze all tracks to get integrated LUFS
                    // Step 2: find Anchor Track (loudest)
                    // Step 3: compute per-track target offsets
                    // This preserves macro-dynamics between tracks.
                    // INV-AB-1: deterministic — same inputs → same targets
                    let global_target = items.first().map(|p| p.target_lufs).unwrap_or(-14.0_f32);

                    let mut track_lufs: Vec<f32> = Vec::with_capacity(total);

                    for params in &items {
                        let (tx, rx) = oneshot::channel();
                        if executor_tx
                            .send(Intent::RunAnalysis {
                                audio_path: params.audio_path.clone(),
                                session_id: params.session_id.clone(),
                                response: tx,
                            })
                            .await
                            .is_err()
                        {
                            track_lufs.push(global_target);
                            continue;
                        }
                        match rx.await {
                            Ok(Ok(analysis)) => {
                                tracing::info!(
                                    batch_id = %batch_id,
                                    "Cohesion pre-pass: {} → {:.1} LUFS",
                                    params.audio_path,
                                    analysis.integrated_lufs
                                );
                                track_lufs.push(analysis.integrated_lufs);
                            }
                            _ => {
                                tracing::warn!(
                                    batch_id = %batch_id,
                                    "Cohesion pre-pass failed for {} — using global target",
                                    params.audio_path
                                );
                                track_lufs.push(global_target);
                            }
                        }
                    }

                    // Anchor = loudest track (highest LUFS = least negative)
                    let anchor_lufs = track_lufs
                        .iter()
                        .copied()
                        .filter(|l| l.is_finite() && *l > -70.0)
                        .fold(f32::NEG_INFINITY, f32::max);

                    let anchor_lufs = if anchor_lufs.is_finite() {
                        anchor_lufs
                    } else {
                        global_target
                    };

                    // per_track_target = global_target - (anchor_lufs - track_lufs)
                    // Anchor → exactly global_target
                    // Quieter tracks → lower target (preserves relative dynamics)
                    let per_track_targets: Vec<f32> = track_lufs
                        .iter()
                        .map(|&lufs| {
                            let offset = anchor_lufs - lufs;
                            (global_target - offset).clamp(-40.0, 0.0)
                        })
                        .collect();

                    tracing::info!(
                        batch_id = %batch_id,
                        "Cohesion: anchor={:.1} LUFS, targets={:?}",
                        anchor_lufs,
                        per_track_targets
                    );
                    // ── End Album Cohesion Pre-Pass ──────────────────────

                    for (index, params) in items.into_iter().enumerate() {
                        let cohesion_target = per_track_targets
                            .get(index)
                            .copied()
                            .unwrap_or(global_target);
                        tracing::info!(
                            batch_id = %batch_id,
                            "Conductor: batch track {}/{} — {}",
                            index + 1, total, params.audio_path
                        );

                        // Build ExecutionPlan for this track
                        let plan = ExecutionPlan {
                            audio_path: params.audio_path.clone(),
                            preset_id: params.preset_id.clone(),
                            target_lufs: cohesion_target,
                            max_tp_db: params.max_tp_db,
                            session_id: params.session_id.clone(),
                        };

                        // Dispatch to Executor (R3) — sequential, await each
                        let (tx, rx) = oneshot::channel();
                        if executor_tx
                            .send(Intent::RunDsp { plan, response: tx })
                            .await
                            .is_err()
                        {
                            outputs.push(super::operator::BatchTrackOutput {
                                track_index: index,
                                session_id: params.session_id,
                                blob_id: String::new(),
                                status: "error",
                                error: Some("Executor channel closed".into()),
                            });
                            continue;
                        }

                        match rx.await {
                            Ok(Ok(dsp_output)) => {
                                outputs.push(super::operator::BatchTrackOutput {
                                    track_index: index,
                                    session_id: params.session_id,
                                    blob_id: dsp_output.blob_id,
                                    status: "ok",
                                    error: None,
                                });
                            }
                            Ok(Err(e)) => {
                                outputs.push(super::operator::BatchTrackOutput {
                                    track_index: index,
                                    session_id: params.session_id,
                                    blob_id: String::new(),
                                    status: "error",
                                    error: Some(format!("{:?}", e)),
                                });
                            }
                            Err(_) => {
                                outputs.push(super::operator::BatchTrackOutput {
                                    track_index: index,
                                    session_id: params.session_id,
                                    blob_id: String::new(),
                                    status: "error",
                                    error: Some("Executor dropped oneshot".into()),
                                });
                            }
                        }
                    }

                    tracing::info!(
                        batch_id = %batch_id,
                        "Conductor: batch complete — {}/{} ok",
                        outputs.iter().filter(|o| o.status == "ok").count(),
                        total
                    );

                    let _ = response.send(Ok(outputs));
                    busy_clone.store(false, Ordering::SeqCst);
                });
            }

            _ => {}
        }
    }
}
