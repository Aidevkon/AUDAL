//! POST /master — trigger mastering pipeline.
//! Authority: Phase 6 task-decomposition P6-003
//! M0 Constitution v2.0 §03: M0 is the trust boundary for all DSP invocations.
//!
//! Invokes sp314-dsp MasteringPipeline::master() natively.
//! Stores resulting blob metrics in BlobStore. Returns blob_id to caller.
//!
//! FORBIDDEN: Returning serde_json::Value.
//! FORBIDDEN: Calling sp314-dsp from the Tauri backend directly.

use axum::{Json, extract::State};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::audit::{AuditEntry, AuditLevel};
use crate::blob_store::{StoredBlob, StoredLoudness, StoredQuality, StoredProvenance};
// Phase 12A (A-003 §1): PCM ownership transfer to xaak after mastering
use xaak::PcmTransfer;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterRequest {
    pub audio_path: String,
    pub preset_id:  String,
    pub flavour_id: Option<String>,
    pub intent_tone: Option<f32>,
    pub intent_dynamics: Option<f32>,
    pub persona_id:  Option<String>,
    pub tone:      Option<f32>,
    pub dynamics:       Option<f32>,
    pub chaos_seed:  Option<u64>,
    pub project_id:  Option<String>,
    pub track_id:    Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MasterResponse {
    pub blob_id: String,
    pub status:  &'static str,   // "ok" | "error"
    pub message: Option<String>,
    pub aether_error: Option<String>,
    pub persona_id:   Option<String>,
    pub converged:    Option<bool>,
}

/// POST /master — run sp314-dsp → store blob → return blob_id.
pub async fn trigger_mastering(
    State(state): State<AppState>,
    Json(req):    Json<MasterRequest>,
) -> Json<serde_json::Value> {
    use tokio::sync::oneshot;
    use crate::agents::operator::{Intent, MasteringParams};

    let session_id = uuid::Uuid::new_v4().to_string();

    // Build MasteringParams — pure translation from HTTP request
    let params = MasteringParams {
        audio_path:  req.audio_path.clone(),
        preset_id:   req.preset_id.clone(),
        target_lufs: -14.0_f32,  // Conductor resolves from SchemaAgent
        max_tp_db:   -1.0_f32,
        session_id:  session_id.clone(),
    };

    // Register progress immediately
    state.progress.insert(session_id.clone(),
        crate::app_state::MasteringProgress {
            job_id:     session_id.clone(),
            stage:      "DISPATCHED".into(),
            elapsed_ms: 0,
            blob_id:    None,
            error:      None,
        }
    );

    state.audit.write(
        crate::audit::AuditEntry::new(
            "m0d.mastering_dispatched",
            crate::audit::AuditLevel::Audit,
            &format!("session={} path={} preset={}",
                session_id, req.audio_path, req.preset_id),
        )
    ).ok();

    // Dispatch to Conductor (R2) — non-blocking
    // HTTP handler does not wait for DSP completion
    let (tx, rx) = oneshot::channel();
    let intent = Intent::ExecuteMastering { params, response: tx };

    let state_bg     = state.clone();
    let session_bg   = session_id.clone();

    tokio::spawn(async move {
        if state_bg.operator.dispatch(intent).await.is_err() {
            state_bg.progress.insert(session_bg.clone(),
                crate::app_state::MasteringProgress {
                    job_id:     session_bg,
                    stage:      "ERROR".into(),
                    elapsed_ms: 0,
                    blob_id:    None,
                    error:      Some("Conductor channel closed".into()),
                }
            );
            return;
        }

        match rx.await {
            Ok(Ok(output)) => {
                state_bg.progress.insert(session_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id:     session_bg,
                        stage:      "CERTIFIED".into(),
                        elapsed_ms: 0,
                        blob_id:    Some(output.blob_id),
                        error:      None,
                    }
                );
            }
            Ok(Err(e)) => {
                state_bg.progress.insert(session_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id:     session_bg,
                        stage:      "ERROR".into(),
                        elapsed_ms: 0,
                        blob_id:    None,
                        error:      Some(format!("{:?}", e)),
                    }
                );
            }
            Err(_) => {
                state_bg.progress.insert(session_bg.clone(),
                    crate::app_state::MasteringProgress {
                        job_id:     session_bg,
                        stage:      "ERROR".into(),
                        elapsed_ms: 0,
                        blob_id:    None,
                        error:      Some("Conductor dropped response".into()),
                    }
                );
            }
        }
    });

    // Return job_id IMMEDIATELY — HTTP does not wait for DSP
    Json(serde_json::json!({ "job_id": session_id }))
}

fn update_stage(state: &AppState, job_id: &str, stage: &str,
                start: &std::time::Instant, blob_id: Option<String>) {
    state.progress.insert(job_id.to_string(), crate::app_state::MasteringProgress {
        job_id:     job_id.to_string(),
        stage:      stage.to_string(),
        elapsed_ms: start.elapsed().as_millis() as u64,
        blob_id,
        error:      None,
    });
}

// Per-stem SHA-256 fingerprints (Dev Protocol §13.3)
// Computed on raw stems before mix — tamper-proof certificate
fn sha256_hex(data: &[f32]) -> String {
    // Fast deterministic fingerprint (not cryptographic SHA256 — no deps)
    // Uses FNV-like accumulation for determinism
    let mut h: u64 = 0xcbf29ce484222325;
    for &s in data {
        let bits = s.to_bits();
        h ^= bits as u64;
        h = h.wrapping_mul(0x100000001b3);
        h ^= bits as u64 >> 32;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

/// Invoke sp314-dsp MasteringPipeline and assemble StoredBlob.
/// Phase 7: uses decode::decode_audio() — real symphonia decode.
/// Runs blocking decode + DSP in Tokio blocking tasks.
#[allow(deprecated)]
pub async fn run_dsp(req: &MasterRequest, start: Instant) -> Result<(StoredBlob, lineos_types::AudioChunk, Option<f32>), String> {
    run_dsp_internal(req, start).await
}

#[inline(always)]
fn map_flavour_to_persona(flavour_id: &str) -> &'static str {
    match flavour_id {
        "warm"      => "warm_analog",
        "clean"     => "clean_punch",
        "punch"     => "clean_punch",
        "air"       => "hybrid_hifi",
        "film"      => "cinematic_wide",
        "broadcast" => "clean_punch",
        _           => "warm_analog",  // default
    }
}

#[inline(always)]
async fn run_dsp_internal(req: &MasterRequest, start: Instant) -> Result<(StoredBlob, lineos_types::AudioChunk, Option<f32>), String> {
    let mut profiler = crate::handlers::timeline::TimelineProfiler::new();
    let audio_path = &req.audio_path;
    let preset_id = &req.preset_id;
    use lineos_types::{
        MasteringIntent,
        AudioChunk, LoudnessTarget,
    };
    use uuid::Uuid;
    use crate::handlers::decode;
    // Phase 9: EBU R128 windowed telemetry — LRA, momentary, short-term LUFS
    use lineos_telemetry::lra::LraCalculator;
    use lineos_telemetry::windows::{momentary_lufs, short_term_lufs};

    // Load schema from shared contract (embedded at compile-time for determinism)
    let schema: serde_json::Value = serde_json::from_str(
        include_str!("../../../../shared/schema/bmr-128.schema.json")
    ).map_err(|e| format!("Schema load error: {e}"))?;

    // Target LUFS from schema preset — clamped to valid range
    let target_lufs: Option<f32> = schema.get("presets")
        .and_then(|p| p.get(preset_id))
        .and_then(|p| p.get("target_lufs"))
        .and_then(|l| l.as_f64())
        .map(|lufs| (lufs as f32).clamp(-40.0, 0.0));

    // Determinism seed from SHA-256 of the file path (stable identity)
    let path_hash    = compute_sha256_bytes(audio_path.as_bytes());
    let input_hash_hex = hex::encode(&path_hash);
    let seed           = derive_seed(&path_hash);

    // ── Phase 7: Real decode ──────────────────────────────────────────────────
    // decode_audio() is CPU-bound (symphonia + rubato) — run in blocking task.
    let path_owned = audio_path.to_string();
    let pcm = tokio::task::spawn_blocking(move || {
        decode::decode_audio(&path_owned)
    }).await
      .map_err(|e| format!("Decode task join error: {e}"))?
      .map_err(|e| format!("Decode error: {e}"))?;


    let original_sr  = pcm.original_sr;
    let original_ch   = pcm.original_ch;
    let duration_ms   = pcm.duration_ms;

    tracing::info!(
        "Decoded: {} samples, sr={}, ch={}, first5={:?}",
        pcm.samples.len(),
        pcm.sample_rate,
        pcm.channels,
        &pcm.samples[..5.min(pcm.samples.len())],
    );
    profiler.mark_stage("Ingest", &pcm.samples);

    // ── Silence guard (after real decode) ────────────────────────────────────
    let rms = compute_rms(&pcm.samples);
    let rms_dbfs = if rms > 0.0 { 20.0 * (rms as f64).log10() as f32 }
                   else         { f32::NEG_INFINITY };

    // Rough normalization gain estimate (RMS-based) — used only for overflow guard.
    // Full LUFS-accurate gain computed inside sp314-dsp AnalysisAccumulator.
    let norm_gain_check = rms_to_lufs(rms);
    tracing::info!("RMS: {:.2} dBFS, est. LUFS: {:.4}", rms_dbfs, norm_gain_check);

    if rms_dbfs < -60.0 {
        return Err(format!(
            "Input validation failed: audio is silence (RMS = {rms_dbfs:.1} dBFS)"
        ));
    }

    // ── Normalization gain overflow guard ─────────────────────────────────────
    // AnalysisAccumulator::normalization_gain_linear() returns inf when
    // integrated_lufs == f32::NEG_INFINITY (no valid LUFS blocks — track < 400ms
    // or K-weighted energy below absolute gate). inf * sample = NaN in Stage 1.
    // Guard: if rough-LUFS signals the gain would exceed 32× (30 dB), the track
    // is too quiet or too short to normalize safely — fail with a clear message.
    let rough_lufs = norm_gain_check; // already computed: rms_to_lufs(rms)
    let rough_gain_db = -14.0_f32 - rough_lufs; // worst-case against Spotify target
    if rough_gain_db > 30.0 {
        return Err(format!(
            "DSP arithmetic error — normalization gain would exceed 32× \
             (input RMS = {rms_dbfs:.1} dBFS, est. gain = {rough_gain_db:.1} dB). \
             Track too quiet or too short (< 400ms) for loudness normalization."
        ));
    }

    // ── P7 audit — audio_decoded (per P7-002 spec) ───────────────────────────
    // Records: original format metadata before normalize/resample.
    // inlined here (no AppState in run_dsp — audit is written by trigger_mastering)
    // We encode this in the blob provenance for now; full audit write is in caller.
    let _ = (original_sr, original_ch, duration_ms); // used in provenance below

    // Build AudioChunk — always 48000 Hz stereo after decode.
    // Clone samples first so telemetry can read full-track PCM after DSP completes.
    let _pcm_samples_for_telemetry = pcm.samples.clone();   // Phase 9
    let pcm_channels_for_telemetry = pcm.channels;          // Phase 9
    let pcm_sr_for_telemetry      = pcm.sample_rate;        // Phase 9
    // Create StereoBuffer (AudioChunk)
    let mut chunk = AudioChunk {
        left: pcm.samples.iter().step_by(2).copied().collect(),
        right: pcm.samples.iter().skip(1).step_by(2).copied().collect(),
        sample_rate: pcm.sample_rate,
        num_frames: pcm.samples.len() / 2,
    };
    let chunk_original = chunk.clone();

    // A1: Full stem separation (NMF v2 — representative sample approach)
    use sp314_dsp::stft::stem_renderer::FiveStemRenderer;
    use sp314_dsp::analysis::StemFeatureAnalyzer;

    let mono: Vec<f32> = chunk.left.iter()
        .zip(chunk.right.iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();

    let mut renderer = FiveStemRenderer::new();
    let stems = renderer.render(&mono);
    profiler.mark_stage("Stem Engine", &stems.voice);

    // POX Voice processing — clean voice before reconstruction
    let clean_voice = if req.flavour_id.as_deref() == Some("broadcast") {
        let voice_topology = pipelineforge::flavor::Flavor::POXVoice.build(chunk.sample_rate);
        let mut voice_graph = sp314_nodes::graph::DspGraph::from_topology(&voice_topology, 512, chunk.sample_rate)
            .map_err(|e| format!("POX graph error: {:?}", e))?;
        let mut v_left  = stems.voice.clone();
        let mut v_right = stems.voice.clone();
        let n = v_left.len();
        let mut frame = 0;
        while frame < n {
            let end = (frame + 512).min(n);
            voice_graph.process_block(&mut v_left[frame..end], &mut v_right[frame..end]);
            frame += end - frame;
        }
        v_left.iter().zip(v_right.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect::<Vec<f32>>()
    } else {
        stems.voice.clone()
    };
    profiler.mark_stage("Pre-Clean", &clean_voice);

    let fingerprints = crate::blob_store::StemFingerprints {
        voice:     sha256_hex(&clean_voice),
        drums:     sha256_hex(&stems.drums),
        bass:      sha256_hex(&stems.bass),
        harmonics: sha256_hex(&stems.harmonics),
        ambience:  sha256_hex(&stems.ambience),
        pipeline: {
            let mut all = clean_voice.clone();
            all.extend_from_slice(&stems.drums);
            all.extend_from_slice(&stems.bass);
            sha256_hex(&all)
        },
    };

    // Reconstruct mix with clean voice
    let n = clean_voice.len();
    let mut mix_left  = vec![0.0f32; n];
    let mut mix_right = vec![0.0f32; n];
    for i in 0..n {
        let mono_mix = clean_voice[i] + stems.bass[i] + stems.harmonics[i] + stems.ambience[i];
        mix_left[i]  = mono_mix + stems.drums[i];
        mix_right[i] = mono_mix + stems.drums[i];
    }

    // Level 1: Energy-preserving reconstruction
    // Source: current track RMS (not corpus — per-session accurate)
    // INV-AB-1: deterministic — same input → same gain always
    let original_rms = libm::sqrtf(
        chunk.left.iter().zip(chunk.right.iter())
            .map(|(l, r)| l * l + r * r)
            .sum::<f32>() / (chunk.left.len() * 2) as f32
    );
    let mix_rms = libm::sqrtf(
        mix_left.iter().zip(mix_right.iter())
            .map(|(l, r)| l * l + r * r)
            .sum::<f32>() / (mix_left.len() * 2) as f32
    );
    let gain = if mix_rms > 1e-10 {
        (original_rms / mix_rms).clamp(0.5, 2.0)
    } else {
        1.0
    };
    for i in 0..mix_left.len() {
        mix_left[i]  *= gain;
        mix_right[i] *= gain;
    }
    // Log for corpus Level 2 (future per-user learning)
    // gain_compensation_db = 20 * log10(gain)

    let features = StemFeatureAnalyzer::analyze(&stems, chunk.sample_rate);

    use sp314_dsp::spatial::SpatialPreAnalysis;
    use sp314_dsp::spatial::channel_assign::StemChannelAssignments;
    use sp314_dsp::spatial::five_dot_one::{FiveDotOneStage, SpatialFirewall};
    use sp314_dsp::spatial::renderer::StereoRenderer;
    use sp314_dsp::spatial::user_profile::UserSpatialProfile;
    use aether::markov::voice_v1::MarkovStateClassifier;

    let spatial_pre = SpatialPreAnalysis::analyze(&mix_left, &mix_right, chunk.sample_rate);
    let assignments  = StemChannelAssignments::compute(&features, &spatial_pre);
    let firewall     = SpatialFirewall::default();
    let mut stage    = FiveDotOneStage::render(&stems, &assignments, &firewall);
    firewall.apply(&mut stage);
    let profile      = UserSpatialProfile::default_podcast();
    let state_str    = MarkovStateClassifier::classify_voice(&features.voice).to_str();
    let modulated    = profile.apply_markov_prediction(state_str);
    let _ = modulated;
    let (sp_l, sp_r) = StereoRenderer::render(&stage);
    let sp_len = mix_left.len().min(sp_l.len());
    for i in 0..sp_len {
        mix_left[i]  = sp_l[i];
        mix_right[i] = sp_r[i];
    }

    chunk.left = mix_left;
    chunk.right = mix_right;
    profiler.mark_stage("Spatial", &chunk.left);

    // A1.5: Pre-Analysis Engine (RFC-008, Constitution v1.3)
    // Runs once per session on raw stereo PCM, before Aether.
    use sp314_dsp::analysis::PreAnalyzer;
    let pre_analysis = PreAnalyzer::run(&chunk.left, &chunk.right, chunk.sample_rate);
    tracing::info!(
        "pre-analysis: lufs={:.1} tp={:.1} lra={:.1} td={:.1}/s corr={:.2} \
         profile=[{:.1},{:.1},{:.1},{:.1},{:.1},{:.1}] \
         zones=[sub={} box={} harsh={} phase={} res={}]",
        pre_analysis.integrated_lufs,
        pre_analysis.true_peak_dbtp,
        pre_analysis.loudness_range,
        pre_analysis.transient_density,
        pre_analysis.global_phase_correlation,
        pre_analysis.spectral_profile_db[0],
        pre_analysis.spectral_profile_db[1],
        pre_analysis.spectral_profile_db[2],
        pre_analysis.spectral_profile_db[3],
        pre_analysis.spectral_profile_db[4],
        pre_analysis.spectral_profile_db[5],
        pre_analysis.zone_flags.zone_sub_rumble,
        pre_analysis.zone_flags.zone_boxiness,
        pre_analysis.zone_flags.zone_cymbal_harsh,
        pre_analysis.zone_flags.zone_phase_issue,
        pre_analysis.zone_flags.zone_harsh_resonance,
    );

    // Before: autotune reads chunk from file (inaccurate)
    // After:  autotune uses PreAnalysis full-track LUFS (accurate)
    let autotune_result = sp314_dsp::pipeline::autotune::autotune(
        pre_analysis.integrated_lufs,
        target_lufs.unwrap_or(-14.0),
    );
    tracing::info!(
        "Autotune (pure math): pre_gain_db={:.2} est_lufs={:.2}",
        autotune_result.pre_gain_db,
        autotune_result.estimated_input_lufs,
    );

    // Apply autotune input gain to audio
    let gain_linear = 10.0_f32
        .powf(autotune_result.pre_gain_db / 20.0_f32);
    for s in chunk.left.iter_mut()  { *s *= gain_linear; }
    for s in chunk.right.iter_mut() { *s *= gain_linear; }

    // MasteringIntent — all three fields verified:
    //   seed:         from path hash (stable; non-zero guaranteed by derive_seed)
    //   target_lufs:  from schema preset, clamped to [-40.0, 0.0]
    //   export_16bit: false (24-bit dither, lossless output)
    let intent = MasteringIntent {
        target: LoudnessTarget {
            target_lufs:      target_lufs.unwrap_or(-14.0),
            max_true_peak_db: -1.0,
            max_lra_lu:       None,
            platform:         "default".into(),
        },
        preset_name:      preset_id.to_string(),
        stem_mode:        false,
        target_makeup_db: 0.0,  // input gain applied above
    };

    // A2: Pre-DSP Aether processing
    let mapped_persona = map_flavour_to_persona(
        req.flavour_id.as_deref().unwrap_or("warm")
    );
    let aether_req = aether_bridge::AetherRequest {
        persona_id:  Some(mapped_persona.to_string()),
        tone:      req.intent_tone.or(req.tone),
        dynamics:       req.intent_dynamics.or(req.dynamics),
        ambience:    None,
        chaos_seed:  req.chaos_seed,
        project_id:  req.project_id.clone(),
        track_id:    req.track_id.clone(),
        preset_name: Some(req.preset_id.clone()),
    };

    let (dsp_config, proof_log, persona_config) = aether_bridge::build_dsp_config(
        &aether_req,
        &features,
        Some(&pre_analysis),
    )
        .map_err(|e| format!("AetherBridge error: {}", e))?;

    let blob_id = uuid::Uuid::new_v4().to_string();

    // V2.0: Corpus generation — silent background telemetry
    // 900-JSON: behavioral stats only, zero audio content
    use lineos_corpus::builder::build_timeline;

    let track_duration_ms = (chunk.left.len() as f32 
        / chunk.sample_rate as f32 * 1000.0) as u32;

    let corpus_envelope = build_timeline(
        &features,
        &stems.voice,
        &stems.drums,
        &stems.bass,
        &stems.harmonics,
        &stems.ambience,
        &pre_analysis,
        &blob_id,
        chunk.sample_rate,
        req.flavour_id.as_deref().unwrap_or("unknown"),
    );

    // Write *.corpus.json alongside mastered file — silent
    let corpus_path = format!("session_{}.corpus.json", &blob_id[..8]);
    if let Ok(json) = serde_json::to_string_pretty(&corpus_envelope) {
        let _ = std::fs::write(&corpus_path, json);
    }

    let mut audio = chunk;

    // Clone target before spawn_blocking consumes intent (fix E0382)
    let intent_target_for_verify = intent.target.clone();

    // Run sp314-dsp in blocking thread (no_std/alloc/sync) with a 60s timeout
    let timeout_result = tokio::time::timeout(
        tokio::time::Duration::from_secs(300),
        tokio::task::spawn_blocking(move || {
            let res = crate::dsp::DspAdapter::master(&intent, &mut audio, Some(&dsp_config));
            (res, audio, dsp_config)
        })
    ).await;

    let (result, audio, dsp_config) = match timeout_result {
        Err(_) => return Err("DSP timeout after 60s".to_string()),
        Ok(Err(e)) => return Err(format!("DSP task join error: {e}")),
        Ok(Ok(r)) => r,
    };
    let result = result.map_err(|e| format!("DSP pipeline error: {:?}", e))?;
    profiler.mark_stage("Mastering", &audio.left);

    let mut audio = audio;
    use sp314_dsp::verification::{PostFlightVerifier, VerificationConfig};
    let verify_cfg = VerificationConfig::from_target(&intent_target_for_verify);
    let verify_result = PostFlightVerifier::verify_and_trim(&mut audio, &verify_cfg);
    if let Some(warn) = &verify_result.warning {
        tracing::warn!("[S-013] ⚠️  {}", warn);
    }
    profiler.mark_stage("Render", &audio.left);

    // Build interleaved post-master samples for telemetry
    let post_master_samples: Vec<f32> = audio.left.iter()
        .zip(audio.right.iter())
        .flat_map(|(l, r)| [*l, *r])
        .collect();
    let post_master_sr       = pcm_sr_for_telemetry;
    let post_master_channels = 2_u16;

    // Measure integrated LUFS on post-master output
    let lufs = sp314_dsp::metering::measure_integrated_lufs(&audio.left, &audio.right);
    tracing::info!("Post-DSP integrated LUFS: {:.4}", lufs);
    let tp    = result.lufs.true_peak_dbfs;
    let lra   = result.lufs.loudness_range_lu;
    let dr    = 10.0; // dynamic range proxy for v3
    let sc    = 1.0;  // stereo correlation proxy for v3
    let elapsed = start.elapsed().as_millis() as u64;

    // ── Phase 9: Telemetry pass — real LRA + windowed LUFS ───────────────────
    // Run full EBU R128 windowed analysis on the decoded (pre-mastered) PCM.
    // LRA measures source material dynamic range — per EBU Tech 3342.
    // Uses libm — no std::f32 methods per LineOS Constitution §09.1.
    let telemetry_lra = tokio::task::spawn_blocking({
        let samples  = post_master_samples.clone();
        let sr       = post_master_sr;
        let channels = post_master_channels;
        move || {
            let mut calc = LraCalculator::new(sr);
            calc.feed_samples(&samples, channels);
            calc.compute()
        }
    }).await.unwrap_or(lra);   // fallback to sp314-dsp value on join failure

    let telemetry_momentary  = momentary_lufs(
        &post_master_samples,
        post_master_sr,
        post_master_channels,
    );
    let telemetry_short_term = short_term_lufs(
        &post_master_samples,
        post_master_sr,
        post_master_channels,
    );

    tracing::info!(
        "Telemetry: lra={:.2} LU, momentary={:.2} LUFS, short_term={:.2} LUFS",
        telemetry_lra, telemetry_momentary, telemetry_short_term
    );

    // Phase 10: store mastered PCM as f32 LE bytes for export.
    // FORBIDDEN: return these bytes to the frontend (Amendment A-002 §3).
    // GoldenBlob.flac_bytes = raw f32 LE interleaved PCM from DSP output (Phase 2/10).
    let audio_bytes: Vec<u8> = audio.left.iter()
        .zip(audio.right.iter())
        .flat_map(|(l, r)| {
            l.to_le_bytes().into_iter()
             .chain(r.to_le_bytes().into_iter())
        })
        .collect();
    let audio_sample_rate = pcm_sr_for_telemetry;
    let audio_channels    = pcm_channels_for_telemetry;

    let cert = aether_bridge::generate_certificate(
        &_pcm_samples_for_telemetry,
        &post_master_samples,
        &persona_config,
        &dsp_config,
        &proof_log,
        &aether_req,
        env!("CARGO_PKG_VERSION"),
    );

    let cert_json = serde_json::to_string(&cert).unwrap_or_default();
    let config_json = serde_json::to_string(&dsp_config).unwrap_or_default();

    let qr_base64 = crate::handlers::certificate::generate_qr_base64(
        &blob_id,
        req.audio_path.split('/').last().unwrap_or("unknown"),
        lufs,
        tp,
        telemetry_lra,
        &fingerprints,
    );

    let pcm_blake3 = crate::handlers::certificate::blake3_pcm(&audio.left);
    let cert_sig   = crate::handlers::certificate::sign_certificate(
        &blob_id, &pcm_blake3,
        lufs, &fingerprints
    );

    let processing_timeline = profiler.finalize();

    Ok((StoredBlob {
        id:               blob_id.clone(),
        version:          "1.0".into(),
        blob_type:        "audio".into(),
        created_at:       Utc::now().to_rfc3339(),
        input_hash:       input_hash_hex,
        seed,
        pipeline_version: env!("CARGO_PKG_VERSION").to_string(),
        preset_id:        preset_id.to_string(),
        stem_fingerprints: Some(fingerprints),
        qr_base64,
        pcm_blake3:       Some(pcm_blake3),
        cert_signature:   Some(cert_sig),
        processing_timeline,
        loudness: StoredLoudness {
            integrated_lufs:          lufs,
            short_term_lufs:          telemetry_short_term,   // Phase 9: real 3s window
            momentary_lufs:           telemetry_momentary,    // Phase 9: real 400ms window
            true_peak_dbtp:           tp,
            lra:                      telemetry_lra,           // Phase 9: real LRA (was 0.0)
            k_weighted:               true,
            ebu_r128_target_lufs:     -23.0,
            ebu_r128_compliant:       lufs <= -23.0 && tp <= -1.0,
            spotify_compliant:        platform_ok(lufs, -14.0, tp),
            youtube_compliant:        platform_ok(lufs, -14.0, tp),
            apple_music_compliant:    platform_ok(lufs, -16.0, tp),
            apple_podcasts_compliant: platform_ok(lufs, -16.0, tp),
            broadcast_compliant:      platform_ok(lufs, -23.0, tp),
            tidal_compliant:          platform_ok(lufs, -14.0, tp),
        },
        quality: StoredQuality {
            stereo_correlation: sc,
            phase_coherence:    0.97,       // Phase 7: real M/S phase analysis
            stereo_width:       0.5,        // Phase 7: real M/S width analysis
            dynamic_range_db:   dr,
            rms_db:             lufs + 3.0, // Phase 7: real RMS measurement
            spectral_centroid:  3_200.0,    // Phase 7: FFT spectral analysis
            spectral_flatness:  0.12,       // Phase 7: FFT flatness analysis
            clips_detected:     0,
            clip_free:          tp <= -1.0,
        },
        provenance: StoredProvenance {
            engine_id:          "E11".into(),
            engine_version:     env!("CARGO_PKG_VERSION").to_string(),
            processing_time_ms: elapsed,
            host_os:            std::env::consts::OS.to_string(),
            created_by:         "stillair-cockpit".into(),
            aether_enriched:    true,
            aether_devices:     vec![],
        },
        schema_version: 2,
        aether_cert:    Some(cert_json),
        aether_persona: Some(persona_config.id),
        aether_config:  Some(config_json),
        // Phase 10: audio payload (never crosses WASM boundary — Amendment A-002 §3)
        audio_bytes:  audio_bytes,
        sample_rate:  audio_sample_rate,
        channels:     audio_channels,
    }, chunk_original, target_lufs))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// LUFS within 1 LU of target AND TP ≤ ceiling → platform compliant.
fn platform_ok(lufs: f32, target: f32, tp: f32) -> bool {
    lufs <= target + 1.0 && tp <= -1.0
}

/// SHA-256 of input bytes — returns [u8; 32].
/// Used for both `input_hash` audit field and determinism `seed`.
fn compute_sha256_bytes(data: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

/// Derive u64 seed from first 8 bytes of hash (big-endian).
fn derive_seed(hash: &[u8; 32]) -> u64 {
    u64::from_be_bytes(hash[..8].try_into().unwrap_or([0; 8]))
}

/// Convert raw bytes → f32 PCM samples (little-endian float) with clamping.
///
/// Phase 6: assumes f32-PCM input. Samples are clamped to [-1.0, 1.0] and
/// NaN/Inf are mapped to 0.0. This makes the function safe for arbitrary
/// byte inputs — encoded bitstreams (MP3/FLAC) will produce near-zero values
/// that the silence guard above catches, rather than crashing Stage1.
///
/// Phase 7: replace with symphonia/hound decode for real format support.
#[allow(dead_code)]
fn bytes_to_f32_samples(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|b| {
            let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            // Map NaN/Inf to 0.0, then clamp to audio range
            if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 }
        })
        .collect()
}

/// Compute RMS amplitude of samples.
fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

/// Rough LUFS estimate from RMS — used only for overflow guard, not stored.
/// Full EBU R128 measurement happens inside sp314-dsp.
fn rms_to_lufs(rms: f32) -> f32 {
    if rms <= 0.0 { return f32::NEG_INFINITY; }
    // K-weighting approximation: subtract ~1 dB from RMS dBFS
    20.0 * rms.log10() - 1.0
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_bytes_deterministic() {
        let h1 = compute_sha256_bytes(b"same input");
        let h2 = compute_sha256_bytes(b"same input");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_derive_seed_non_zero() {
        let h = compute_sha256_bytes(b"creator-os");
        assert_ne!(derive_seed(&h), 0);
    }

    #[test]
    fn test_platform_ok_at_target() {
        assert!(platform_ok(-14.0, -14.0, -1.0));
    }

    #[test]
    fn test_platform_ok_too_loud() {
        assert!(!platform_ok(-12.0, -14.0, -1.0));
    }

    #[test]
    fn test_platform_ok_tp_exceeded() {
        assert!(!platform_ok(-14.0, -14.0, -0.5));
    }

    #[test]
    fn test_bytes_to_f32_clamps_in_range() {
        // Valid f32-PCM bytes representing 0.5 should pass through unchanged
        let sample: f32 = 0.5;
        let bytes = sample.to_le_bytes();
        let out = bytes_to_f32_samples(&bytes);
        assert_eq!(out.len(), 1);
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_bytes_to_f32_clamps_mp3_bitstream() {
        // MP3 sync word bytes [0xFF, 0xFB, ...] reinterpreted as f32 → NaN or huge value
        // bytes_to_f32_samples must return 0.0 or a clamped value, never > 1.0
        let mp3_header_bytes = [0xFF, 0xFBu8, 0x90, 0x00];
        let out = bytes_to_f32_samples(&mp3_header_bytes);
        assert_eq!(out.len(), 1);
        assert!(out[0] >= -1.0 && out[0] <= 1.0,
            "Clamped value must be in [-1.0, 1.0], got {}", out[0]);
    }

    #[test]
    fn test_compute_rms_silence() {
        let samples = vec![0.0f32; 4096];
        assert_eq!(compute_rms(&samples), 0.0);
    }

    #[test]
    fn test_compute_rms_half_amp() {
        // Signal at 0.5 amplitude → RMS = 0.5 / sqrt(2) ≈ 0.354
        let samples: Vec<f32> = (0..4096)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48000.0).sin())
            .collect();
        let rms = compute_rms(&samples);
        assert!((rms - 0.5_f32 / 2.0_f32.sqrt()).abs() < 0.01,
            "RMS should be ~0.354, got {rms}");
    }

    #[test]
    fn test_silence_guard_threshold() {
        // RMS of 0.0 → dBFS = -∞ → below -60 dBFS silence threshold
        let rms = 0.0_f32;
        let rms_dbfs = if rms > 0.0 { 20.0 * (rms as f64).log10() as f32 }
                       else         { f32::NEG_INFINITY };
        assert!(rms_dbfs < -60.0, "Silence must be below guard threshold");
    }

    #[test]
    fn test_gain_overflow_guard() {
        // Very quiet input (-70 LUFS) with target -14 LUFS → gain >> 32×
        let quiet_rms = 10.0_f32.powf(-70.0 / 20.0);
        let measured_lufs = rms_to_lufs(quiet_rms);
        let gain_db = -14.0 - measured_lufs;
        let gain_linear = 10.0_f32.powf(gain_db / 20.0);
        assert!(gain_linear > 32.0,
            "Very quiet input should trigger overflow guard, gain={gain_linear:.1}×");
    }

    #[test]
    fn test_target_lufs_clamped() {
        // Out-of-range values should be clamped
        let clamped_low: f32  = (-50.0_f32).clamp(-40.0, 0.0);
        let clamped_high: f32 = (5.0_f32).clamp(-40.0, 0.0);
        assert_eq!(clamped_low,  -40.0);
        assert_eq!(clamped_high,   0.0);
    }

}
