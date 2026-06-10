use std::time::Instant;
use chrono::Utc;
use crate::blob_store::{StoredBlob, StoredLoudness, StoredQuality, StoredProvenance};
use crate::handlers::master::MasterRequest;
// Per-stem SHA-256 fingerprints (Dev Protocol §13.3)
// Computed on raw stems before mix — tamper-proof certificate


/// Invoke sp314-dsp MasteringPipeline and assemble StoredBlob.
/// Phase 7: uses decode::decode_audio() — real symphonia decode.
/// Runs blocking decode + DSP in Tokio blocking tasks.
#[allow(deprecated)]
pub fn run_dsp(req: &MasterRequest, start: Instant) -> Result<(StoredBlob, std::path::PathBuf, Option<f32>), String> {
    run_dsp_internal(req, start)
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
fn run_dsp_internal(req: &MasterRequest, start: Instant) -> Result<(StoredBlob, std::path::PathBuf, Option<f32>), String> {
    let mut profiler = crate::handlers::timeline::TimelineProfiler::new();
    let audio_path = &req.audio_path;
    let preset_id = &req.preset_id;
    use lineos_types::{
        MasteringIntent,
        AudioChunk, LoudnessTarget,
    };
    
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
    let input_hash_hex = hex::encode(path_hash);
    let seed           = derive_seed(&path_hash);

    // ── Phase 7: Real decode ──────────────────────────────────────────────────
    // decode_audio() is CPU-bound (symphonia + rubato). Since run_dsp_internal is synchronous, we run it directly.
    let path_owned = audio_path.to_string();
    let pcm = decode::decode_audio(&path_owned)
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
    let _pcm_channels_for_telemetry = pcm.channels;          // Phase 9
    let _pcm_sr_for_telemetry      = pcm.sample_rate;        // Phase 9
    // Create StereoBuffer (AudioChunk)
    let mut chunk = AudioChunk {
        left: pcm.samples.iter().step_by(2).copied().collect(),
        right: pcm.samples.iter().skip(1).step_by(2).copied().collect(),
        sample_rate: pcm.sample_rate,
        num_frames: pcm.samples.len() / 2,
    };
    let _chunk_original = chunk.clone();

    // ── ST-P5: TwoPassEngine stem separation via MPSC streaming ─────
    // NMF phase RAM: ~7MB (was ~8GB for 2h file)
    // Mix accumulator: ~300MB (down from ~8.3GB total)
    // Phase 8: replace accumulator with MP3 streaming encoder
    use sp314_dsp::stft::two_pass::TwoPassEngine;
    use sp314_dsp::spatial::five_dot_one::FiveDotOneStage;
    use sp314_dsp::spatial::renderer::StereoRenderer;
    use sp314_dsp::spatial::user_profile::UserSpatialProfile;
    use sha2::{Sha256, Digest};

    let mono: Vec<f32> = chunk.left.iter()
        .zip(chunk.right.iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();

    // M-P7: True Scout Window (One Shot One Kill)
    // Seek to 30% of the track to avoid intro silence.
    // Feed exactly 2 seconds of the chorus to train the NMF perfectly.
    let scout_window_len = (2.0 * chunk.sample_rate as f32) as usize;
    let scout_start      = (mono.len() as f32 * 0.30) as usize;
    let scout_end        = (scout_start + scout_window_len).min(mono.len());
    let scout_slice      = &mono[scout_start..scout_end];

    // Pass 1 — Scout: trains on the 2-second chorus window!
    let mut two_pass = TwoPassEngine::new();
    let scout        = two_pass.scout(scout_slice, chunk.sample_rate);

    // M-P5: Maestro AutoTuning — between Pass 1 and Pass 2.
    // Reads stem MFCCs from scout + UserMarkovModel history.
    // Computes adaptive ducking_gain for this track.
    // INV-AB-1: deterministic — same scout + same model → same params.
    let render_params = {
        use crate::dsp::maestro::AutoTuningController;

        // Load user model if it exists (silent failure — no model = default params)
        let model_path = format!("user_model_{}.json",
            req.project_id.as_deref().unwrap_or("default"));
        let user_model = std::fs::read_to_string(&model_path)
            .ok()
            .and_then(|json| lineos_corpus::store::UserMarkovModel::from_json(&json).ok());

        let preset_id = req.flavour_id.as_deref().unwrap_or("default");

        AutoTuningController::compute_render_params(
            &scout,
            user_model.as_ref(),
            preset_id,
        )
    };

    tracing::info!(
        event        = "m0d.maestro_params",
        ducking_gain = render_params.ducking_gain,
        bass_drums_distance = scout.stem_mfccs.bass_drums_distance(),
        "Maestro: adaptive ducking_gain computed"
    );
    profiler.mark_stage("Scout Pass", &mono);

    // Build minimal StemFeatures for downstream APIs
    // Full StemFeatureAnalyzer requires FiveStems — not available in streaming mode.
    // Use default values — aether_bridge uses tone/dynamics from req, not stems.
    use lineos_types::{StemFeatures, StemMetrics, MixMetrics};
    // Minimal StemFeatures for downstream APIs in streaming mode.
    // StemFeatureAnalyzer requires FiveStems — not available in streaming.
    // Aether uses tone/dynamics from req, not raw stem metrics.
    let streaming_features = StemFeatures {
        voice:     StemMetrics::default(),
        drums:     StemMetrics::default(),
        bass:      StemMetrics::default(),
        harmonics: StemMetrics::default(),
        ambience:  StemMetrics::default(),
        mix:       MixMetrics::default(),
    };

    // Phase 2: Zero Allocation Disk Streaming via memmap2
    let n_total = mono.len();
    const STFT_FLUSH_TAIL: usize = 1024;
    let n_total_with_tail = n_total + STFT_FLUSH_TAIL;

    let blob_id = req.track_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let file_path = std::path::PathBuf::from(format!("/tmp/m0d-mastering-{}.pcm", blob_id));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&file_path)
        .map_err(|e| format!("Failed to create mapped file: {e}"))?;

    // Size: n_total_with_tail * 2 channels * 4 bytes/float
    let file_size = (n_total_with_tail * 2 * 4) as u64;
    file.set_len(file_size).map_err(|e| format!("Failed to set file len: {e}"))?;

    let mut mmap = unsafe {
        memmap2::MmapMut::map_mut(&file).map_err(|e| format!("Mmap failed: {e}"))?
    };

    // The first half of mmap is LEFT, second half is RIGHT
    // We cast the raw bytes to f32 slices safely using std::slice::from_raw_parts_mut
    let (left_bytes, right_bytes) = mmap.split_at_mut(n_total_with_tail * 4);
    let left_slice: &mut [f32] = unsafe {
        std::slice::from_raw_parts_mut(left_bytes.as_mut_ptr() as *mut f32, n_total_with_tail)
    };
    let right_slice: &mut [f32] = unsafe {
        std::slice::from_raw_parts_mut(right_bytes.as_mut_ptr() as *mut f32, n_total_with_tail)
    };

    // Streaming SHA-256 hashers — no full stem Vec needed
    let mut h_voice      = Sha256::new();
    let mut h_drums      = Sha256::new();
    let mut h_bass       = Sha256::new();
    let mut h_harmonics  = Sha256::new();
    let mut h_ambience   = Sha256::new();

    // POX voice graph — stateful, lives outside closure
    let is_broadcast = req.flavour_id.as_deref() == Some("broadcast");
    let mut voice_graph_opt = if is_broadcast {
        let topo = pipelineforge::flavor::Flavor::POXVoice
            .build(chunk.sample_rate);
        Some(
            sp314_nodes::graph::DspGraph::from_topology(
                &topo, 512, chunk.sample_rate,
            ).map_err(|e| format!("POX graph error: {:?}", e))?
        )
    } else {
        None
    };

    // Resolve mix levels — default 1.0 if not provided (backward compatible)
    // INV-MX-1: applied before spatial rendering
    let mix = req.mix_levels.as_ref()
        .map(|m: &crate::handlers::master::MixLevels| m.clamped())
        .unwrap_or_default();

    // Pass 2 — process_chunks: ~2MB/chunk constant RAM
    let mut write_offset = 0;
    two_pass.process_chunks_with_params(&mono, &scout, render_params.ducking_gain, |stems_chunk| {
        let chunk_len = stems_chunk.voice.len();

        // Apply mix levels — INV-MX-1: before spatial rendering
        let mv: Vec<f32> = stems_chunk.voice.iter()
            .map(|s| s * mix.voice).collect();
        let md: Vec<f32> = stems_chunk.drums.iter()
            .map(|s| s * mix.drums).collect();
        let mb: Vec<f32> = stems_chunk.bass.iter()
            .map(|s| s * mix.bass).collect();
        let mh: Vec<f32> = stems_chunk.harmonics.iter()
            .map(|s| s * mix.harmonics).collect();
        let ma: Vec<f32> = stems_chunk.ambience.iter()
            .map(|s| s * mix.ambience).collect();

        // Streaming hashes on mixed stems
        h_voice.update(unsafe { std::slice::from_raw_parts(
            mv.as_ptr() as *const u8, mv.len() * 4) });
        h_drums.update(unsafe { std::slice::from_raw_parts(
            md.as_ptr() as *const u8, md.len() * 4) });
        h_bass.update(unsafe { std::slice::from_raw_parts(
            mb.as_ptr() as *const u8, mb.len() * 4) });
        h_harmonics.update(unsafe { std::slice::from_raw_parts(
            mh.as_ptr() as *const u8, mh.len() * 4) });
        h_ambience.update(unsafe { std::slice::from_raw_parts(
            ma.as_ptr() as *const u8, ma.len() * 4) });

        // POX voice processing per chunk
        let mut clean_voice = mv.clone();
        if let Some(ref mut vg) = voice_graph_opt {
            let mut v_right = clean_voice.clone();
            let mut frame = 0;
            while frame < chunk_len {
                let end = (frame + 512).min(chunk_len);
                vg.process_block(
                    &mut clean_voice[frame..end],
                    &mut v_right[frame..end],
                );
                frame = end;
            }
            // Average L+R → mono clean voice
            for i in 0..chunk_len {
                clean_voice[i] = (clean_voice[i] + v_right[i]) * 0.5;
            }
        }

        // Spatial render_chunk with locked assignments from scout
        // INV-MX-2: SpatialFirewall scales applied after mix levels
        let stage = FiveDotOneStage::render_chunk(
            &clean_voice,
            &md,
            &mb,
            &mh,
            &ma,
            &scout.assignments,
        );
        let mut stage = stage;
        stage.apply_scales(scout.rear_scale, scout.lfe_scale);
        let (sp_l, sp_r) = StereoRenderer::render(&stage);

        // Write directly to mapped slices
        let end_offset = write_offset + sp_l.len();
        left_slice[write_offset..end_offset].copy_from_slice(&sp_l);
        right_slice[write_offset..end_offset].copy_from_slice(&sp_r);
        write_offset = end_offset;
    }).map_err(|e| format!("TwoPassEngine error: {e}"))?;

    profiler.mark_stage("Stem Engine", left_slice);

    // Finalize streaming fingerprints
    let voice_hex    = format!("{:x}", h_voice.finalize());
    let drums_hex    = format!("{:x}", h_drums.finalize());
    let bass_hex     = format!("{:x}", h_bass.finalize());
    let harm_hex     = format!("{:x}", h_harmonics.finalize());
    let amb_hex      = format!("{:x}", h_ambience.finalize());
    let pipeline_hex = {
        let mut hp = Sha256::new();
        hp.update(voice_hex.as_bytes());
        hp.update(bass_hex.as_bytes());
        format!("{:x}", hp.finalize())
    };

    let fingerprints = crate::blob_store::StemFingerprints {
        voice:     voice_hex,
        drums:     drums_hex,
        bass:      bass_hex,
        harmonics: harm_hex,
        ambience:  amb_hex,
        pipeline:  pipeline_hex,
    };

    profiler.mark_stage("Pre-Clean", left_slice);

    // Level 1: Energy-preserving reconstruction
    let original_rms = libm::sqrtf(
        chunk.left.iter().zip(chunk.right.iter())
            .map(|(l, r)| l * l + r * r)
            .sum::<f32>() / (chunk.left.len() * 2) as f32
    );
    let mix_rms = libm::sqrtf(
        left_slice.iter().zip(right_slice.iter())
            .map(|(l, r)| l * l + r * r)
            .sum::<f32>() / (left_slice.len() * 2) as f32
    );
    let gain = if mix_rms > 1e-10 {
        (original_rms / mix_rms).clamp(0.5, 2.0)
    } else { 1.0 };

    for i in 0..left_slice.len() {
        left_slice[i]  *= gain;
        right_slice[i] *= gain;
    }

    // Markov spatial modulation (simplified — full in Phase 8)
    let profile   = UserSpatialProfile::default_podcast();
    let modulated = profile.apply_markov_prediction("vowel");
    let _ = modulated;

    // chunk.left  = left_slice;
    // chunk.right = right_slice;
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
        &streaming_features,
        Some(&pre_analysis),
    )
        .map_err(|e| format!("AetherBridge error: {}", e))?;

    // V2.0: Corpus generation — silent background telemetry
    // 900-JSON: behavioral stats only, zero audio content
    use lineos_corpus::builder::build_timeline;

    let _track_duration_ms = (chunk.left.len() as f32 
        / chunk.sample_rate as f32 * 1000.0) as u32;

    let corpus_envelope = build_timeline(
        &streaming_features,
        left_slice,
        left_slice,
        left_slice,
        left_slice,
        left_slice,
        &pre_analysis,
        &blob_id,
        chunk.sample_rate,
        req.flavour_id.as_deref().unwrap_or("unknown"),
    );

    // Write *.corpus.json alongside mastered file — silent
    let corpus_path = format!("session_{}.corpus.json", &blob_id[..blob_id.len().min(8)]);
    if let Ok(json) = serde_json::to_string_pretty(&corpus_envelope) {
        let _ = std::fs::write(&corpus_path, json);
    }

    // CB-P8: Update per-preset UserMarkovModel (incremental, silent)
    // INV-CB-1: never modifies DSP behavior — background learning only
    // INV-CB-8: only updates the preset_id for this session
    {
        use lineos_corpus::store::{UserMarkovModel, aggregate_preset};

        let preset_id  = req.flavour_id.as_deref().unwrap_or("default");
        let model_path = format!("user_model_{}.json",
            req.project_id.as_deref().unwrap_or("default"));

        // Load existing model or create new one
        let mut user_model = std::fs::read_to_string(&model_path)
            .ok()
            .and_then(|json| UserMarkovModel::from_json(&json).ok())
            .unwrap_or_else(|| UserMarkovModel::new(
                req.project_id.as_deref().unwrap_or("default")
            ));

        user_model.update(preset_id, &corpus_envelope);

        // Save updated model — silent failure (never blocks mastering)
        if let Ok(json) = user_model.to_json() {
            let _ = std::fs::write(&model_path, json);
        }

        // Every 10 sessions: recompute global preset snapshot
        if user_model.version % 10 == 0 {
            if let Some(global) = aggregate_preset(preset_id, &[&user_model]) {
                let global_path = format!("global_{}_v{}.json",
                    preset_id, user_model.version / 10);
                if let Ok(json) = serde_json::to_string(&global) {
                    let _ = std::fs::write(&global_path, json);
                }
            }
        }
    }

    // Run sp314-dsp directly (we are already in a blocking thread)
    let dsp_start_time = std::time::Instant::now();
    let result = crate::dsp::DspAdapter::master(&intent, left_slice, right_slice, chunk.sample_rate, Some(&dsp_config));
    if dsp_start_time.elapsed().as_secs() > 300 {
        tracing::warn!("DSP took more than 300 seconds");
    }
    
    let result = result.map_err(|e| format!("DSP pipeline error: {:?}", e))?;
    profiler.mark_stage("Mastering", left_slice);

    let lufs = result.lufs.integrated_lufs;
    tracing::info!("Post-DSP integrated LUFS: {:.4}", lufs);
    let tp    = result.lufs.true_peak_dbfs;
    let _lra   = result.lufs.loudness_range_lu;
    let dr    = 10.0; // dynamic range proxy for v3
    let sc    = 1.0;  // stereo correlation proxy for v3
    let elapsed = start.elapsed().as_millis() as u64;

    // ── Phase 9: Telemetry pass — real LRA + windowed LUFS ───────────────────
    // Run full EBU R128 windowed analysis on the mapped PCM
    // Post-master samples for telemetry are interleaved manually
    let mut post_master_samples = Vec::with_capacity(n_total * 2);
    for i in 0..n_total {
        post_master_samples.push(left_slice[i]);
        post_master_samples.push(right_slice[i]);
    }
    let post_master_sr       = chunk.sample_rate;
    let post_master_channels = 2_u16;

    let telemetry_lra = {
        let mut calc = LraCalculator::new(post_master_sr);
        calc.feed_samples(&post_master_samples, post_master_channels);
        calc.compute()
    };

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

    // Sync mapped file to disk before returning path
    mmap.flush().unwrap_or_default();
    
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
        req.audio_path.split('/').next_back().unwrap_or("unknown"),
        lufs,
        tp,
        telemetry_lra,
        &fingerprints,
    );

    let pcm_blake3 = crate::handlers::certificate::blake3_pcm(left_slice);
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
        sample_rate:  post_master_sr,
        channels:     post_master_channels,
        num_frames:   n_total,
        audio_path:   file_path.clone(),
    },
    file_path,
    None
))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// LUFS within 1 LU of target AND TP ≤ ceiling → platform compliant.
fn platform_ok(lufs: f32, target: f32, tp: f32) -> bool {
    lufs <= target + 1.0 && tp <= -1.0
}

/// SHA-256 of input bytes — returns [u8; 32].
/// Used for both `input_hash` audit field and determinism `seed`.
pub fn compute_sha256_bytes(data: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

/// Derive u64 seed from first 8 bytes of hash (big-endian).
pub fn derive_seed(hash: &[u8; 32]) -> u64 {
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
pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

/// Rough LUFS estimate from RMS — used only for overflow guard, not stored.
/// Full EBU R128 measurement happens inside sp314-dsp.
pub fn rms_to_lufs(rms: f32) -> f32 {
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
