//! Executor — R3: executes DSP plan. Pure action, no decisions.
//! Receives ExecutionPlan from Conductor. Calls run_dsp_internal().
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.3
//! Motto: "I do not think. I do."
//!
//! run_dsp_internal() is the entire DSP pipeline — decode, NMF,
//! spatial, mastering, certification. Executor wraps it in
//! spawn_blocking and returns DspOutput to Conductor.

use super::operator::{DspOutput, ExecutorError, Intent};
use crate::domain::content_type::ContentTypeExt;
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
                // Daemon-owned output path — matching v2's pattern (episode_render's
                // pcm_path derives from blob_id the same way). The caller no longer
                // supplies this: an unsanitized caller-supplied path was a
                // trust/traversal gap, and the UI has no concept of this field
                // anyway (MasterRequest never carried one for v2 either).
                let output_path = format!("/tmp/m0d-v3-streaming-{}.wav", blob_id);
                let raw_tap_path = format!("/tmp/m0d-raw-{}.pcm", blob_id);

                let path_hash =
                    crate::domain::dsp_pipeline::compute_sha256_bytes(audio_path.as_bytes());
                let input_hash_hex_path = hex::encode(path_hash);
                let seed = crate::domain::dsp_pipeline::derive_seed(&path_hash);
                let cert_start = std::time::Instant::now();

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

                    let scout_out = crate::domain::nodes::scout_node::run(
                        &left,
                        &right,
                        sample_rate,
                        "default",
                        plan.flavour_id.as_deref().unwrap_or("default"),
                        &pre_analysis,
                    )
                    .map_err(|e| ExecutorError::DspFailed(format!("scout failed: {e}")))?;
                    let streaming_features = scout_out.scout.features.clone();

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

                    // Full-file input LUFS measurement — the missing pre-gain
                    // step for v3 (Parts A+B: LoudnessTarget::from_preset +
                    // measure_input_lufs, combined here with the same closed-form
                    // formula v2 uses). Runs on the ORIGINAL input (not the
                    // standardized stream used for rendering) so it reflects the
                    // true source material.
                    let target_lufs = plan.target_lufs_override.unwrap_or_else(|| {
                        lineos_types::config::LoudnessTarget::from_preset(&plan.preset_id).target_lufs
                    });
                    let input_lufs = crate::dsp::input_lufs::measure_input_lufs(path)
                        .map_err(|e| ExecutorError::DspFailed(format!("input LUFS measurement failed: {e}")))?;
                    let pre_gain_linear = match input_lufs {
                        Some(measured) => {
                            let result = sp314_dsp::pipeline::autotune::autotune(measured, target_lufs);
                            libm::powf(10.0, result.pre_gain_db / 20.0)
                        }
                        None => 1.0,
                    };

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
                    // Decoder stack: Tapped(Standardized(file)).
                    // StandardizedDecoder guarantees 48k/stereo/
                    // sanitized chunks (the same contract v2 and the
                    // Episode path enforce) and computes the
                    // input-identity hashes internally; TappedDecoder
                    // tees those standardized chunks to the raw A/B
                    // dump — so the A/B "raw" is exactly what entered
                    // the DSP, which is the honest comparison.
                    let std_decoder =
                        crate::dsp::standardized_decoder::StandardizedDecoder::open(
                            std::path::Path::new(&audio_path),
                        )
                        .map_err(|e| {
                            ExecutorError::DspFailed(format!(
                                "standardized decode open failed: {e}"
                            ))
                        })?;
                    let main_decoder = sp314_orchestrator::decode_provider::TappedDecoder::new(
                        std_decoder,
                        raw_tap_path.clone(),
                    );

                    // 3. Call run_streaming_pipeline_with_timeline
                    let frames_written = sp314_orchestrator::streaming_pipeline::run_streaming_pipeline_with_timeline(
                        &main_decoder,
                        &output_path,
                        &sp314_orchestrator::streaming_pipeline::StreamingConfig {
                            topology: &ducking_topology,
                            block_size: 1024,
                            sample_rate: 48_000, // StandardizedDecoder's output rate — the DSP graph must be built for what it will actually receive, not the file's native rate
                            ducking_node_id: "duck_gain",
                            speech_gain: 1.0,
                            music_gain: 0.501,
                            pre_gain_linear,
                        },
                        sp314_orchestrator::streaming_pipeline::TimelinePlan {
                            boundaries,
                            flagged_indices,
                            pre_analysis: Some(&pre_analysis),
                        },
                        rx_res,
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
                    let measured =
                        crate::dsp::wav_to_raw::wav_to_raw_measured(&output_path, &mastered_raw_path)
                            .map_err(|e| {
                                ExecutorError::DspFailed(format!("wav→raw post-pass failed: {e}"))
                            })?;

                    // Two independent counts of the same quantity: the pipeline
                    // counted frames while WRITING the WAV; the measured pass
                    // counted frames while READING it back. They must agree.
                    if frames_written != measured.frames_written {
                        return Err(ExecutorError::DspFailed(format!(
                            "frame count mismatch: pipeline wrote {} but measured pass read {}",
                            frames_written, measured.frames_written
                        )));
                    }

                    let (_input_blake3, input_sha256) = main_decoder.inner().input_hashes();

                    let icfg = crate::domain::nodes::dsp_node::build_intent_and_config(
                        &plan.preset_id,
                        plan.flavour_id.as_deref().unwrap_or("default"),
                        plan.intent_tone,
                        plan.intent_dynamics,
                        None, // chaos_seed — same
                        None, // project_id — same
                        None, // track_id — same
                        None, // target_lufs — streaming default
                        &streaming_features,
                        &pre_analysis,
                    )
                    .map_err(ExecutorError::DspFailed)?;
                    let (fingerprints, spatial_metadata) =
                        crate::domain::content_type::ContentType::bypassed_render();
                    let cert_data = crate::domain::nodes::certificate_node::StreamingCertData {
                        pcm_blake3: measured.pcm_blake3.clone(),
                        output_sha256: measured.output_sha256.clone(),
                        dead_air: Default::default(), // decoder consumed by reference; into_dead_air needs ownership — deferred, honest default (rescue roster)
                    };
                    let cert_out = crate::domain::nodes::certificate_node::run_streaming(
                        &blob_id,
                        measured.output_lufs,
                        measured.output_lra,
                        measured.true_peak_dbtp,
                        &fingerprints,
                        &spatial_metadata,
                        &icfg.proof_log,
                        &icfg.persona_config,
                        &icfg.aether_req,
                        &icfg.dsp_config,
                        std::path::PathBuf::from(&output_path),
                        &input_hash_hex_path,
                        48_000,
                        cert_start.elapsed().as_millis() as u64,
                        seed,
                        &plan.preset_id,
                        input_sha256,
                        measured.frames_written,
                        Vec::new(), // processing_timeline — v3 has no TimelineProfiler yet (rescue roster)
                        cert_data,
                    )
                    .map_err(ExecutorError::DspFailed)?;

                    // 4. Real StreamingOutput + Blob
                    Ok((
                        crate::agents::operator::StreamingOutput {
                            job_id,
                            blob_id: blob_id.clone(),
                            status: "certified",
                            pcm_data: Some(mastered_raw_path.clone()),
                            num_frames: measured.frames_written,
                            sample_rate: 48_000,
                            pcm_blake3: measured.pcm_blake3.clone(),
                            output_lufs: measured.output_lufs,
                        },
                        cert_out.blob,
                    ))
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
                        blob_store.insert(blob);
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

                    // bpm: DEFERRED, not forgotten. The streaming
                    // BeatDetector needed for a real value here is
                    // a planned necessity (see NEST: it will be
                    // built for the Kepler UI instrument regardless
                    // of this task), not YAGNI — but building it is
                    // out of scope for this migration. Confirmed via
                    // recon that bpm is pure passthrough in album
                    // cohesion today (dead-ends at UI telemetry,
                    // never drives any DSP/certificate decision), so
                    // 0.0 here changes no real behavior versus what
                    // v3's own RunStreaming arm already reports
                    // (which is also frequently 0.0 today, per a
                    // separate, already-logged finding).
                    let bpm = 0.0;

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
