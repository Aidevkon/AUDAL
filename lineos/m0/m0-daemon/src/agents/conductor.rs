//! Conductor — R2: orchestrates mastering workflow.
//! Receives MasteringParams from HTTP handler.
//! Builds ExecutionPlan. Dispatches RunDsp to Executor.
//! Collects DspOutput. Returns MasteringOutput.
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.2
//! Motto: "I build the plan. I do not execute it."

use tokio::sync::{mpsc, oneshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use super::operator::{
    Intent, ConductorError, ExecutorError,
    ExecutionPlan, MasteringOutput, DspOutput,
};

pub async fn run(mut rx: mpsc::Receiver<Intent>) {
    // AtomicBool: only one mastering job at a time
    // R2 decision: is the system busy?
    let busy = Arc::new(AtomicBool::new(false));

    // Conductor holds its own channel to Executor
    // Created once at startup — persists for the lifetime of the agent
    let (executor_tx, executor_rx) = mpsc::channel::<Intent>(4);
    tokio::spawn(super::executor::run(executor_rx));

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
                let busy_clone  = busy.clone();

                // Spawn so HTTP handler is not blocked
                tokio::spawn(async move {
                    // R2: build ExecutionPlan from MasteringParams
                    // Pure translation — no business logic on values
                    let plan = ExecutionPlan {
                        audio_path:  params.audio_path,
                        preset_id:   params.preset_id,
                        target_lufs: params.target_lufs,
                        max_tp_db:   params.max_tp_db,
                        session_id:  params.session_id.clone(),
                    };

                    // Dispatch to Executor (R3)
                    let (tx, rx) = oneshot::channel();
                    let run_dsp_intent = Intent::RunDsp {
                        plan,
                        response: tx,
                    };

                    if executor_tx.send(run_dsp_intent).await.is_err() {
                        let _ = response.send(Err(
                            ConductorError::ExecutorFailed(
                                "Executor channel closed".into()
                            )
                        ));
                        busy_clone.store(false, Ordering::SeqCst);
                        return;
                    }

                    // Await Executor result
                    match rx.await {
                        Err(_) => {
                            let _ = response.send(Err(
                                ConductorError::ExecutorFailed(
                                    "Executor dropped oneshot".into()
                                )
                            ));
                        }
                        Ok(Err(ExecutorError::DspFailed(e))) => {
                            let _ = response.send(Err(
                                ConductorError::ExecutorFailed(e)
                            ));
                        }
                        Ok(Err(ExecutorError::BlobStoreFailed(e))) => {
                            let _ = response.send(Err(
                                ConductorError::ExecutorFailed(e)
                            ));
                        }
                        Ok(Ok(dsp_output)) => {
                            // R2: assemble MasteringOutput from DspOutput
                            let output = MasteringOutput {
                                job_id:  params.session_id,
                                blob_id: dsp_output.blob_id,
                                status:  "ok",
                            };
                            let _ = response.send(Ok(output));
                        }
                    }

                    // Release busy flag
                    busy_clone.store(false, Ordering::SeqCst);
                });
            }

            _ => {}
        }
    }
}
