//! Conductor — R2: orchestrates mastering workflow.
//! Receives MasteringParams from HTTP handler.
//! Builds ExecutionPlan. Dispatches RunDsp to Executor.
//! Collects DspOutput. Returns MasteringOutput.
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.2
//! Motto: "I build the plan. I do not execute it."

use super::operator::{ConductorError, ExecutionPlan, ExecutorError, Intent, MasteringOutput};
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use xaak::repo::DspState;

// allow: 8 args; a params-struct refactor is deliberately deferred — not done as a clippy side-fix
#[allow(clippy::too_many_arguments)]
pub async fn run(
    mut rx: mpsc::Receiver<Intent>,
    head_state_ptr: Arc<ArcSwap<DspState>>,
    db: crate::db::DbConn,
    blob_store: crate::blob_store::BlobStore,
    album_tx: tokio::sync::broadcast::Sender<crate::app_state::AlbumEvent>,
    progress_tx: tokio::sync::broadcast::Sender<crate::app_state::MasteringProgress>,
    progress_map: Arc<dashmap::DashMap<String, crate::app_state::MasteringProgress>>,
    config: Arc<crate::config::M0Config>,
) {
    // AtomicBool: only one mastering job at a time
    // R2 decision: is the system busy?
    let busy = Arc::new(AtomicBool::new(false));

    // Conductor holds its own channel to Executor
    // Created once at startup — persists for the lifetime of the agent
    let (executor_tx, executor_rx) = mpsc::channel::<Intent>(4);
    tokio::spawn(super::executor::run(
        executor_rx,
        head_state_ptr.clone(),
        db.clone(),
        blob_store.clone(),
        progress_tx,
        progress_map.clone(),
        config.state_path.clone(),
        config.masters_path.clone(),
    ));

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
                        mix_levels: params.mix_levels,
                        normalizer_ceiling_db: params.normalizer_ceiling_db,
                        use_nmfd: params.use_nmfd,
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
                                sample_rate: dsp_output.sample_rate,
                                raw_pcm_data: dsp_output.raw_pcm_data,
                                lufs: dsp_output.lufs,
                                true_peak: dsp_output.true_peak,
                                persisted_master: dsp_output.persisted_master,
                            };
                            let _ = response.send(Ok(output));
                        }
                    }

                    // Release busy flag
                    busy_clone.store(false, Ordering::SeqCst);
                });
            }

            Intent::ExecuteStreaming { params, response } => {
                // R2 decision: busy check
                if busy.swap(true, Ordering::SeqCst) {
                    let _ = response.send(Err(ConductorError::Busy));
                    continue;
                }

                let executor_tx = executor_tx.clone();
                let busy_clone = busy.clone();

                // Spawn so HTTP handler is not blocked
                tokio::spawn(async move {
                    // R2: build StreamingPlan from StreamingParams
                    // Pure translation — no business logic on values
                    let plan = super::operator::StreamingPlan {
                        audio_path: params.audio_path,
                        preset_id: params.preset_id,
                        flavour_id: params.flavour_id,
                        intent_tone: params.intent_tone,
                        intent_dynamics: params.intent_dynamics,
                        target_lufs_override: params.target_lufs_override,
                        session_id: params.session_id,
                    };

                    // Dispatch to Executor (R3)
                    let (tx, rx) = oneshot::channel();
                    let run_streaming_intent = Intent::RunStreaming { plan, response: tx };

                    if executor_tx.send(run_streaming_intent).await.is_err() {
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
                        Ok(Ok(streaming_output)) => {
                            let _ = response.send(Ok(streaming_output));
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
                if busy.swap(true, Ordering::SeqCst) {
                    let _ = response.send(Err(ConductorError::Busy));
                    continue;
                }

                let album_tx = album_tx.clone();
                let head_state_ptr = head_state_ptr.clone();
                let blob_store = blob_store.clone();
                let busy_clone = busy.clone();
                let config_clone = config.clone();

                tokio::spawn(async move {
                    let result = super::batch::run_batch(
                        &batch_id,
                        items,
                        head_state_ptr,
                        &blob_store,
                        &config_clone.certs_path,
                        &album_tx,
                    )
                    .await;

                    let _ = response.send(result);
                    busy_clone.store(false, Ordering::SeqCst);
                });
            }

            _ => {}
        }
    }
}
