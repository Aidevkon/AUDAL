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
    state_dir: String,
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
                let s_dir = state_dir.clone();
                let result = tokio::task::spawn_blocking(move || {
                    crate::domain::dsp_pipeline::run_dsp(
                        &req,
                        start,
                        head_state,
                        Some(p_tx),
                        Some(p_map),
                        j_id,
                        &s_dir,
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
                    Ok(Ok((blob, spatial_blob_opt, mastered_path, user_model_opt))) => {
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

                        if let Some(spatial_blob) = spatial_blob_opt {
                            blob_store.insert(spatial_blob.clone());
                            eprintln!("[SPATIAL] persisted spatial blob: {}", spatial_blob.id);
                        }

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

            Intent::RunStreaming { plan, response } => {
                // R3: pure extraction + execution.
                let audio_path = plan.audio_path.clone();
                let job_id = plan.session_id.clone();
                let blob_id = uuid::Uuid::new_v4().to_string();
                let raw_tap_path = format!("/tmp/m0d-raw-{}.pcm", blob_id);

                let result = tokio::task::spawn_blocking(move || {
                    let path = std::path::Path::new(&audio_path);

                    // 1. Read the 30s scout sample
                    let (left, right, sample_rate) =
                        crate::dsp::lazy_reader::read_scout_sample(path, 30.0).ok_or_else(
                            || ExecutorError::DspFailed("Failed to read 30s scout sample".into()),
                        )?;

                    // 2. Run PreAnalyzer
                    let mut pre_analysis = sp314_dsp::analysis::pre_analysis::PreAnalyzer::run(
                        &left,
                        &right,
                        sample_rate,
                    );

                    // 3. If content suggests Music (TODO: define genre/content-type decision), run BeatDetector
                    // TODO: Decide if genre is Music before running BeatDetector
                    let mono: Vec<f32> = left
                        .iter()
                        .zip(right.iter())
                        .map(|(l, r)| (*l + *r) * 0.5)
                        .collect();
                    let beat_detector = crate::dsp::beat_detector::BeatDetector::new(sample_rate);
                    let (bpm, beats_ms, downbeats_ms, transients_ms) = beat_detector.analyze(&mono);

                    pre_analysis.bpm = bpm;
                    pre_analysis.beats_ms = beats_ms;
                    pre_analysis.downbeats_ms = downbeats_ms;
                    pre_analysis.transients_ms = transients_ms;

                    // 4. Print/log as a temporary checkpoint
                    eprintln!(
                        "RunStreaming PreAnalysis Checkpoint: genre={:?} bpm={}",
                        pre_analysis.genre, pre_analysis.bpm
                    );

                    // a. Build boundaries
                    let decoder = crate::dsp::file_decoder::FileDecoder {
                        path: audio_path.clone(),
                    };
                    let boundaries = sp314_orchestrator::pass1_pipeline::build_timeline_map(
                        decoder,
                    )
                    .map_err(|e| {
                        ExecutorError::DspFailed(format!("Failed to build timeline map: {}", e))
                    })?;

                    // b. Build minimal ducking topology
                    let mut db =
                        sp314_nodes::topology::DspTopologyBuilder::new("ducking_fallback_topology");
                    let d_in = db.add_node("in", "Input", serde_json::json!({}));
                    let d_gain = db.add_node(
                        "duck_gain",
                        "Gain",
                        serde_json::json!({ "gain": 1.0, "glide_ms": 300.0 }),
                    );
                    let d_out = db.add_node("out", "Output", serde_json::json!({}));

                    db.connect(&d_in, &d_gain);
                    db.connect(&d_gain, &d_out);
                    let ducking_topology = db.build();

                    // c. Temporary eprintln!
                    eprintln!(
                        "RunStreaming Topology Checkpoint: {} boundaries, Fallback Graph nodes: {}",
                        boundaries.len(),
                        ducking_topology.nodes.len(),
                    );

                    // 1. Construct NMF channels and spawn worker
                    let (tx_job, rx_job) = std::sync::mpsc::channel();
                    let (tx_res, rx_res) = std::sync::mpsc::channel();
                    let shadow_reader = crate::dsp::lazy_reader::LazyAudioReader::open(
                        std::path::Path::new(&audio_path),
                    )
                    .map_err(|e| {
                        ExecutorError::DspFailed(format!("Failed to open shadow reader: {}", e))
                    })?;

                    let _worker_handle = crate::dsp::orchestrator::nmf_worker::spawn(
                        shadow_reader,
                        sample_rate,
                        rx_job,
                        tx_res,
                    );

                    let (_, flagged_indices) =
                        crate::dsp::orchestrator::nmf_worker::dispatch_all_jobs(
                            &boundaries,
                            &tx_job,
                        );

                    // 2. Construct fresh FileDecoder
                    let main_decoder = sp314_orchestrator::decode_provider::TappedDecoder::new(
                        crate::dsp::file_decoder::FileDecoder {
                            path: audio_path.clone(),
                        },
                        raw_tap_path.clone(),
                    );

                    // 3. Call run_streaming_pipeline_with_timeline
                    let output_path = plan.output_path.clone();
                    let frames_written = sp314_orchestrator::streaming_pipeline::run_streaming_pipeline_with_timeline(
                        &main_decoder,
                        &output_path,
                        &ducking_topology, // Passing minimal ducking fallback graph
                        1024,
                        sample_rate,
                        boundaries,
                        "duck_gain", // ducking_node_id
                        1.0,         // speech_gain
                        0.501,       // music_gain (-6dB)
                        Some(&pre_analysis),
                        rx_res,
                        flagged_indices,
                    )
                    .map_err(|e| {
                        ExecutorError::DspFailed(format!("Streaming pipeline failed: {}", e))
                    })?;

                    if let Some(e) = main_decoder.take_tap_error() {
                        eprintln!(
                            "[V3] raw A/B tap failed (best-effort, master unaffected): {e}"
                        );
                    }
                    let mastered_raw_path =
                        std::path::PathBuf::from(format!("/tmp/m0d-mastered-{}.pcm", blob_id));
                    crate::dsp::wav_to_raw::wav_to_raw_pcm(&output_path, &mastered_raw_path)
                        .map_err(|e| {
                            ExecutorError::DspFailed(format!("wav→raw post-pass failed: {e}"))
                        })?;

                    // 4. Real StreamingOutput
                    Ok(crate::agents::operator::StreamingOutput {
                        job_id,
                        blob_id,
                        status: "completed",
                        pcm_data: Some(std::path::PathBuf::from(output_path)),
                        num_frames: frames_written,
                        sample_rate,
                    })
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
                    Ok(Ok(output)) => {
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
