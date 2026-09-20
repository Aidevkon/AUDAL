//! Executor — R3: executes DSP plan. Pure action, no decisions.
//! Receives ExecutionPlan from Conductor. Calls run_dsp_internal().
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.3
//! Motto: "I do not think. I do."
//!
//! run_dsp_internal() is the entire DSP pipeline — decode, NMF,
//! spatial, mastering, certification. Executor wraps it in
//! spawn_blocking and returns DspOutput to Conductor.
//!
//! `execute_streaming_plan` itself moved to conformance 21/09
//! (F-137/PRD §6.1) — it returns the unsigned certificate as a
//! value, and `Intent::RunStreaming` below signs it via
//! `certificate_node::sign_and_render` before returning.

use super::operator::{DspOutput, ExecutorError, Intent};
use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::mpsc;
use xaak::repo::DspState;

pub async fn run(
    mut rx: mpsc::Receiver<Intent>,
    head_state_ptr: Arc<ArcSwap<DspState>>,
    _db: crate::db::DbConn,
    blob_store: crate::blob_store::BlobStore,
    progress_tx: tokio::sync::broadcast::Sender<crate::app_state::MasteringProgress>,
    progress_map: Arc<dashmap::DashMap<String, crate::app_state::MasteringProgress>>,
    state_dir: String,
    masters_dir: String,
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
                    mix_levels: plan.mix_levels,
                    normalizer_ceiling_db: plan.normalizer_ceiling_db,
                    preview_id: None,
                    restoration_enabled: None,
                    macro_router_enabled: None,
                    vad_observe_enabled: None,
                    use_nmfd: plan.use_nmfd,
                };

                let start = std::time::Instant::now();

                // spawn_blocking: DSP is CPU-intensive, must not block async runtime
                let head_state = head_state_ptr.clone();
                let p_tx = progress_tx.clone();
                let p_map = progress_map.clone();
                let j_id = plan.session_id.clone();
                let s_dir = state_dir.clone();
                let m_dir = masters_dir.clone();
                let result = tokio::task::spawn_blocking(move || {
                    crate::domain::dsp_pipeline::run_dsp(
                        &req,
                        start,
                        head_state,
                        Some(p_tx),
                        Some(p_map),
                        j_id,
                        &s_dir,
                        &m_dir,
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
                    Ok(Ok((
                        blob,
                        spatial_blob_opt,
                        mastered_path,
                        user_model_opt,
                        raw_guard_opt,
                        artifacts,
                    ))) => {
                        // Executor: persist UserMarkovModel to ~/.creator_os/state/
                        // Zero file I/O in DSP layer — this is the correct layer
                        // Executor: persist UserMarkovModel — single overwrite
                        if let Some(ref model) = user_model_opt {
                            let _ = std::fs::create_dir_all(&state_dir);
                            if let Ok(json) = model.to_json() {
                                let model_path = format!("{}/user_model_corpus.json", state_dir);
                                let _ = std::fs::write(&model_path, json);
                            }
                        }

                        // Track write removed here — ExecutionPlan lacks project_id/track_id.
                        // Moved to the HTTP handler that calls run_dsp/handles the request.

                        blob_store.insert(blob.clone());

                        if let Some(spatial_blob) = spatial_blob_opt {
                            blob_store.insert(spatial_blob.clone());
                            eprintln!("[SPATIAL] persisted spatial blob: {}", spatial_blob.core.id);
                        }

                        let p = crate::app_state::MasteringProgress {
                            job_id: plan.session_id.clone(),
                            stage: "CERTIFIED".into(),
                            elapsed_ms: start.elapsed().as_millis() as u64,
                            blob_id: Some(blob.core.id.clone()),
                            error: None,
                            bpm: None,
                        };
                        progress_map.insert(plan.session_id.clone(), p.clone());
                        let _ = progress_tx.send(p);

                        let l = blob.loudness()
                            .expect("executor requires certified blob");
                        let output = DspOutput {
                            blob_id: blob.core.id.clone(),
                            lufs: l.integrated_lufs,
                            true_peak: l.true_peak_dbtp,
                            pcm_data: Some(mastered_path),
                            num_frames: blob.core.num_frames,
                            sample_rate: blob.core.sample_rate,
                            raw_pcm_data: raw_guard_opt,
                            persisted_master: artifacts.persisted_master,
                        };
                        let _ = response.send(Ok(output));
                    }
                }
            }

            Intent::RunStreaming { plan, response } => {
                let progress_tx_clone = progress_tx.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let (output, blob) =
                        conformance::executor::execute_streaming_plan(&plan, Some(progress_tx_clone))?;
                    // The engine returns the unsigned declaration — sign_and_render
                    // stays here because it needs identity.rs/handlers::certificate/
                    // handlers::pdf_gen, which stay with the server (§7 απόφαση 3).
                    let file_path = std::sync::Arc::new(lineos_types::audio::ManagedPcm::new(
                        crate::spool::spool_dir()
                            .join(format!("m0d-v3-streaming-{}.wav", blob.core.id)),
                    ));
                    let mut cert_out =
                        crate::domain::nodes::certificate_node::CertificateOutput { blob, file_path };
                    crate::domain::nodes::certificate_node::sign_and_render(&mut cert_out)
                        .map_err(ExecutorError::DspFailed)?;
                    Ok((output, cert_out.blob))
                })
                .await;

                match result {
                    Err(e) => {
                        let _ = response.send(Err(ExecutorError::DspFailed(format!(
                            "spawn_blocking join error: {e}"
                        ))));
                    }
                    Ok(Err(e)) => {
                        let _ = response.send(Err(e));
                    }
                    Ok(Ok((output, blob))) => {
                        blob_store.insert(blob.clone());
                        let _ = response.send(Ok(output));
                    }
                }
            }

            Intent::RunAnalysis {
                audio_path,
                session_id,
                response,
            } => {
                let result = tokio::task::spawn_blocking(move || {
                    // R3: streaming measurement only. No DSP. No
                    // decisions. Migrated off decode_smart's
                    // whole-file-in-memory read (a real risk for
                    // long audiobooks/podcasts) to the same
                    // StandardizedAudioStream pass the v3 render
                    // path already uses — one full-file read,
                    // O(1) memory, matching this codebase's
                    // established streaming pattern.
                    let path = std::path::Path::new(&audio_path);
                    let metrics =
                        crate::dsp::input_lufs::measure_input_metrics(path).map_err(|e| {
                            super::operator::ExecutorError::DspFailed(format!(
                                "input measurement failed: {e}"
                            ))
                        })?;
                    // -144.0 matches PreAnalysisData::silent()'s
                    // existing codebase convention for "too short
                    // to gate" — not a new sentinel invented here.
                    let integrated_lufs = metrics.integrated_lufs.unwrap_or(-144.0);

                    // Real, full-file BPM via measure_input_metrics's
                    // StreamingBeatDetector integration (2026-07-19) — the
                    // previously-deferred value now landed. This is TELEMETRY-ONLY,
                    // same as everywhere else this bpm is used; it does not drive
                    // any DSP decision (album cohesion's bpm remains pure
                    // passthrough, confirmed via recon in c85f56a).
                    let bpm = metrics.bpm;

                    Ok(super::operator::AnalysisResult {
                        session_id,
                        integrated_lufs,
                        true_peak_dbtp: metrics.true_peak_dbtp,
                        bpm,
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
