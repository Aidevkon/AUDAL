//! Conductor — R2: orchestrates mastering workflow.
//! Receives MasteringParams from HTTP handler.
//! Builds ExecutionPlan. Dispatches RunDsp to Executor.
//! Collects DspOutput. Returns MasteringOutput.
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.2
//! Motto: "I build the plan. I do not execute it."

use super::operator::{ConductorError, ExecutionPlan, ExecutorError, Intent, MasteringOutput};
use crate::domain::nodes::album_certificate_node::AlbumCertificate;
use arc_swap::ArcSwap;
use sp314_dsp::analysis::ear_fatigue::EarFatigueModel;
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
                // R2 decision: busy check
                if busy.swap(true, Ordering::SeqCst) {
                    let _ = response.send(Err(ConductorError::Busy));
                    continue;
                }

                let executor_tx = executor_tx.clone();
                let album_tx = album_tx.clone();
                let head_state_ptr = head_state_ptr.clone();
                let busy_clone = busy.clone();
                let config_clone = config.clone();

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
                    let mut track_analyses: Vec<super::operator::AnalysisResult> =
                        Vec::with_capacity(total);

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
                            track_analyses.push(super::operator::AnalysisResult {
                                session_id: params.session_id.clone(),
                                integrated_lufs: global_target,
                                true_peak_dbtp: 0.0,
                                bpm: 0.0,
                            });
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
                                track_analyses.push(analysis);
                            }
                            _ => {
                                tracing::warn!(
                                    batch_id = %batch_id,
                                    "Cohesion pre-pass failed for {} — using global target",
                                    params.audio_path
                                );
                                track_lufs.push(global_target);
                                track_analyses.push(super::operator::AnalysisResult {
                                    session_id: params.session_id.clone(),
                                    integrated_lufs: global_target,
                                    true_peak_dbtp: 0.0,
                                    bpm: 0.0,
                                });
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

                    // EarFatigue uses integrated_lufs as proxy
                    // Full PreAnalysisData available in pre-pass above
                    let ear_model = EarFatigueModel::default();
                    // Build proxy analyses from track_lufs for EarFatigue
                    let proxy_analyses: Vec<lineos_types::pre_analysis::PreAnalysisData> =
                        track_lufs
                            .iter()
                            .map(|&lufs| lineos_types::pre_analysis::PreAnalysisData {
                                integrated_lufs: lufs,
                                ..lineos_types::pre_analysis::PreAnalysisData::silent()
                            })
                            .collect();

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

                        let ear_delta = if index > 0 {
                            proxy_analyses
                                .get(index - 1)
                                .map(|prev| ear_model.compute_delta(prev))
                                .unwrap_or_default()
                        } else {
                            sp314_dsp::analysis::ear_fatigue::EarFatigueDelta::default()
                        };

                        if ear_delta.fatigue_detected {
                            tracing::info!(
                                batch_id = %batch_id,
                                "EarFatigue: track {} recovery (prev LUFS={:.1})",
                                index + 1,
                                track_lufs.get(index.saturating_sub(1)).copied().unwrap_or(-14.0),
                            );
                        }
                        // AB-P6: Apply EarFatigue delta to DspState via ArcSwap.
                        // Audio thread reads new state on next frame — zero dropout.
                        if ear_delta.fatigue_detected {
                            let current = head_state_ptr.load_full();
                            let adjusted = DspState {
                                ducking_depth: (current.ducking_depth
                                    * ear_delta.ducking_multiplier)
                                    .clamp(0.3, 1.0),
                                ms_width: (current.ms_width * ear_delta.width_multiplier)
                                    .clamp(0.5, 2.0),
                                sidechain_hold: current.sidechain_hold,
                                lfe_gain: current.lfe_gain,
                            };
                            head_state_ptr.store(std::sync::Arc::new(adjusted));
                            tracing::info!(
                                batch_id = %batch_id,
                                "AB-P6: EarFatigue ArcSwap — ducking={:.3} width={:.3}",
                                adjusted.ducking_depth,
                                adjusted.ms_width,
                            );
                        }

                        let ducking = if ear_delta.fatigue_detected {
                            let current = head_state_ptr.load();
                            (current.ducking_depth * ear_delta.ducking_multiplier).clamp(0.3, 1.0)
                        } else {
                            1.0
                        };
                        let bpm = track_analyses.get(index).map(|a| a.bpm).unwrap_or(0.0);
                        let _ = album_tx.send(crate::app_state::AlbumEvent::PreAnalysis {
                            track: index + 1,
                            bpm,
                            ducking_gain: ducking,
                        });

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

                    let ok_count = outputs.iter().filter(|o| o.status == "ok").count();
                    tracing::info!(
                        batch_id = %batch_id,
                        "Conductor: batch complete — {}/{} ok",
                        ok_count, total
                    );

                    // AB-P7: Generate AlbumCertificate from batch results
                    // Build minimal StoredBlob proxies from outputs
                    let fatigue_map: Vec<bool> = proxy_analyses
                        .windows(2)
                        .map(|w| {
                            let model =
                                sp314_dsp::analysis::ear_fatigue::EarFatigueModel::default();
                            model.compute_delta(&w[0]).fatigue_detected
                        })
                        .chain(std::iter::once(false))
                        .collect();

                    let album_cert_path = {
                        // Build proxy blobs from track_lufs + blob_ids
                        let proxy_blobs: Vec<crate::blob_store::StoredBlob> = outputs
                            .iter()
                            .zip(track_lufs.iter())
                            .map(|(o, &lufs)| crate::blob_store::StoredBlob {
                                id: o.blob_id.clone(),
                                input_hash: o.session_id.clone(),
                                loudness: crate::blob_store::StoredLoudness {
                                    integrated_lufs: lufs,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .collect();

                        let anchor_idx = proxy_analyses
                            .iter()
                            .enumerate()
                            .max_by(|(_, a), (_, b)| {
                                a.integrated_lufs.partial_cmp(&b.integrated_lufs).unwrap()
                            })
                            .map(|(i, _)| i)
                            .unwrap_or(0);

                        let cert = AlbumCertificate::from_tracks(
                            &batch_id,
                            &proxy_blobs,
                            anchor_idx,
                            &fatigue_map,
                        );
                        let path = cert.write_to_disk(&batch_id, &config_clone.certs_path);
                        tracing::info!(
                            batch_id = %batch_id,
                            "AB-P7: AlbumCertificate written → {:?}",
                            path
                        );
                        path
                    };
                    let _ = album_cert_path;

                    let _ = response.send(Ok(outputs));
                    busy_clone.store(false, Ordering::SeqCst);
                });
            }

            _ => {}
        }
    }
}
