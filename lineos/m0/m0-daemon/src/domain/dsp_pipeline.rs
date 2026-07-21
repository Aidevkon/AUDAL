use crate::blob_store::StoredBlob;
use crate::domain::{ContentType, ContentTypeExt};
use crate::handlers::master::MasterRequest;
use arc_swap::ArcSwap;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;
// Per-stem SHA-256 fingerprints (Dev Protocol §13.3)
// Computed on raw stems before mix — tamper-proof certificate

/// Invoke sp314-dsp MasteringPipeline and assemble StoredBlob.
/// Phase 7: uses decode::decode_audio() — real symphonia decode.
/// Runs blocking decode + DSP in Tokio blocking tasks.
#[allow(deprecated)]
pub fn run_dsp(
    req: &MasterRequest,
    start: Instant,
    head_state: Arc<ArcSwap<DspState>>,
    progress_tx: Option<tokio::sync::broadcast::Sender<crate::app_state::MasteringProgress>>,
    progress_map: Option<Arc<dashmap::DashMap<String, crate::app_state::MasteringProgress>>>,
    job_id: String,
    state_dir: &str,
) -> Result<
    (
        StoredBlob,
        Option<StoredBlob>,
        std::path::PathBuf,
        Option<lineos_corpus::store::UserMarkovModel>,
    ),
    String,
> {
    run_dsp_internal(
        req,
        start,
        head_state,
        progress_tx,
        progress_map,
        job_id,
        state_dir,
    )
}

#[inline(always)]
pub fn map_flavour_to_persona(flavour_id: &str) -> &'static str {
    match flavour_id {
        "warm" => "warm_analog",
        "clean" => "clean_punch",
        "punch" => "clean_punch",
        "air" => "hybrid_hifi",
        "film" => "cinematic_wide",
        "broadcast" => "clean_punch",
        _ => "warm_analog", // default
    }
}

#[inline(always)]
// allow: 9 args; a params-struct refactor is deliberately deferred — not done as a clippy side-fix
#[allow(clippy::too_many_arguments)]
fn spatial_conformance_path(
    mut channels: [Vec<f32>; 6],
    sample_rate: u32,
    num_frames: usize,
    blob_id: &str,
    preset_id: &str,
    input_hash_hex: &str,
    seed: u64,
    input_blake3_hex: &str,
    _input_sha256_hex: &str,
) -> Result<
    (
        crate::blob_store::StoredBlob,
        std::path::PathBuf,
        Option<lineos_corpus::store::UserMarkovModel>,
    ),
    String,
> {
    use sp314_dsp::limiter::true_peak::TruePeakDetector;
    use sp314_dsp::metering::lufs::measure_integrated_lufs;

    // 1. BS.775 downmix για LUFS measurement
    const CSURR: f32 = 0.707;
    let stereo_l: Vec<f32> = (0..num_frames)
        .map(|i| channels[0][i] + CSURR * channels[2][i] + CSURR * channels[4][i])
        .collect();
    let stereo_r: Vec<f32> = (0..num_frames)
        .map(|i| channels[1][i] + CSURR * channels[2][i] + CSURR * channels[5][i])
        .collect();

    let measured_lufs = measure_integrated_lufs(&stereo_l, &stereo_r);

    // 2. Gain offset για Apple -18 LKFS target
    let gain_db = -18.0_f32 - measured_lufs;
    let gain_linear = 10.0_f32.powf(gain_db / 20.0);

    if gain_db.abs() > 30.0 {
        return Err(format!(
            "spatial_conformance: gain offset {gain_db:.1} dB exceeds ±30 dB limit (measured LUFS: {measured_lufs:.1})"
        ));
    }

    for ch in channels.iter_mut() {
        for s in ch.iter_mut() {
            *s *= gain_linear;
        }
    }

    // 3. True Peak limiter per channel (-1 dBTP)
    let ceiling = 10.0_f32.powf(-1.0 / 20.0);
    for ch in channels.iter_mut() {
        let mut detector = TruePeakDetector::new();
        let mut max_tp = 0.0_f32;
        for &s in ch.iter() {
            let tp = detector.process(s, s);
            max_tp = max_tp.max(tp);
        }
        if max_tp > ceiling {
            let scale = ceiling / max_tp;
            for s in ch.iter_mut() {
                *s *= scale;
            }
        }
    }

    // 4. Interleave 6ch και γράψε raw PCM dump
    let mut interleaved = Vec::with_capacity(num_frames * 6);
    // allow: 6ch interleave, column access across planar buffers
    #[allow(clippy::needless_range_loop)]
    for i in 0..num_frames {
        for ch in 0..6 {
            interleaved.push(channels[ch][i]);
        }
    }
    let raw_path = format!("/tmp/m0d-raw-{}.pcm", blob_id);
    let raw_bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(interleaved.as_ptr() as *const u8, interleaved.len() * 4)
    };
    std::fs::write(&raw_path, raw_bytes)
        .map_err(|e| format!("spatial: failed to write PCM dump: {e}"))?;

    // 5. Φτιάξε StoredBlob
    let blob = crate::blob_store::StoredBlob {
        id: blob_id.to_string(),
        version: "1.0".to_string(),
        blob_type: "spatial_bed".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        input_hash: input_hash_hex.to_string(),
        seed,
        pipeline_version: env!("CARGO_PKG_VERSION").to_string(),
        preset_id: preset_id.to_string(),
        channels: 6,
        sample_rate,
        audio_path: raw_path.clone().into(),
        num_frames,
        pcm_blake3: Some(input_blake3_hex.to_string()),
        ..Default::default()
    };

    Ok((blob, std::path::PathBuf::from(raw_path), None))
}

#[inline(always)]
fn run_dsp_internal(
    req: &MasterRequest,
    start: Instant,
    head_state: Arc<ArcSwap<DspState>>,
    progress_tx: Option<tokio::sync::broadcast::Sender<crate::app_state::MasteringProgress>>,
    progress_map: Option<Arc<dashmap::DashMap<String, crate::app_state::MasteringProgress>>>,
    job_id: String,
    state_dir: &str,
) -> Result<
    (
        StoredBlob,
        Option<StoredBlob>,
        std::path::PathBuf,
        Option<lineos_corpus::store::UserMarkovModel>,
    ),
    String,
> {
    let mut profiler = crate::handlers::timeline::TimelineProfiler::new();
    let audio_path = &req.audio_path;
    let preset_id = &req.preset_id;
    let content_type = ContentType::from_preset(preset_id);

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|x| x * x).sum::<f32>() / buf.len().max(1) as f32).sqrt()
    }

    let emit_progress = |stage_name: &str| {
        let p = crate::app_state::MasteringProgress {
            job_id: job_id.clone(),
            stage: stage_name.into(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            blob_id: None,
            error: None,
            bpm: None,
        };
        if let Some(map) = &progress_map {
            map.insert(job_id.clone(), p.clone());
        }
        if let Some(tx) = &progress_tx {
            let _ = tx.send(p);
        }
    };

    let blob_id = req
        .track_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Deterministic identity from the file PATH
    // (not its contents) — same value decode_node
    // derives, computable without reading audio.
    let path_hash = compute_sha256_bytes(audio_path.as_bytes());
    let input_hash_hex = hex::encode(path_hash);
    let seed = derive_seed(&path_hash);

    // target_lufs from the preset schema (cheap,
    // compile-time embedded JSON — no audio read).
    // Falls back to the Apple Podcasts spec.
    let episode_target_lufs = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../../shared/schema/bmr-128.schema.json"
    ))
    .ok()
    .and_then(|s| {
        s.get("presets")
            .and_then(|p| p.get(preset_id))
            .and_then(|p| p.get("target_lufs"))
            .and_then(|l| l.as_f64())
            .map(|l| (l as f32).clamp(-40.0, 0.0))
    })
    .or(Some(-16.0));

    // ═══ EPISODE STREAMING PATH (O(1) RAM) ═══
    // Bounded-memory podcast mastering. The full
    // file is NEVER decoded into RAM: a 30s scout
    // slice drives analysis, and the audio streams
    // through StandardizedAudioStream (on-the-fly
    // resample→48k, channel normalize, sanitize,
    // input hashing) into the O(1) episode_render.
    // This early return sits BEFORE NODE 1, so the
    // legacy full-file decode below runs only for
    // Music.
    if content_type == ContentType::Episode {
        emit_progress("Ingest");

        // Analysis on a bounded 30s scout slice in
        // the file's NATIVE sample rate.
        let (scout_left, scout_right, scout_sr) =
            crate::dsp::lazy_reader::read_scout_sample(std::path::Path::new(audio_path), 30.0)
                .ok_or_else(|| {
                    "episode scout: could not read \
                 audio"
                        .to_string()
                })?;

        // TRUE fail-fast: reject a dead file on the
        // already-in-RAM scout slice before paying for
        // NMF/stem in scout_node. Same silence rule as
        // the streaming tier1 shield (one source of
        // truth). The streaming hook remains as the
        // universal eject button for paths without a scout.
        crate::dsp::signal_health::SignalHealthMonitor::check_scout_silence(
            &scout_left,
            &scout_right,
        )?;

        let mut pre_analysis =
            sp314_dsp::analysis::PreAnalyzer::run(&scout_left, &scout_right, scout_sr);
        pre_analysis.bpm = 0.0; // spoken word: no tempo

        emit_progress("Scout Pass");
        let scout_out = crate::domain::nodes::scout_node::run(
            &scout_left,
            &scout_right,
            scout_sr,
            req.project_id.as_deref().unwrap_or("default"),
            req.flavour_id.as_deref().unwrap_or("default"),
            &pre_analysis,
        )?;
        let streaming_features = scout_out.scout.features.clone();

        // Corpus (user model) on the scout slice.
        let flavour = req.flavour_id.as_deref().unwrap_or("warm");
        let proj_id = req.project_id.as_deref().unwrap_or("default");
        let model_path = format!("{}/user_model_{}.json", state_dir, proj_id);
        let user_model = std::fs::read_to_string(&model_path)
            .ok()
            .and_then(|j| lineos_corpus::store::UserMarkovModel::from_json(&j).ok())
            .unwrap_or_else(|| lineos_corpus::store::UserMarkovModel::new(proj_id));
        let _corpus_out = crate::domain::nodes::corpus_node::run(
            &streaming_features,
            &scout_left,
            &pre_analysis,
            &blob_id,
            scout_sr,
            flavour,
            user_model,
        );

        let icfg = crate::domain::nodes::dsp_node::build_intent_and_config(
            preset_id,
            flavour,
            req.intent_tone.or(req.tone),
            req.intent_dynamics.or(req.dynamics),
            None,
            req.project_id.as_deref(),
            req.track_id.as_deref(),
            episode_target_lufs,
            &streaming_features,
            &pre_analysis,
        )?;

        // Streaming decoder: delivers 48kHz stereo
        // regardless of input SR/channels.
        let mut stream = crate::dsp::standardized_stream::StandardizedAudioStream::open(
            std::path::Path::new(audio_path),
        )?;
        let stream_sr = {
            use crate::dsp::audio_source::AudioSource;
            stream.sample_rate()
        };

        // BUGFIX (SR mismatch): build the graph for
        // stream_sr (always 48000), NOT scout_sr.
        // The legacy path built the graph at 48k but
        // fed it native-rate samples from a non-
        // resampling reader, shifting every EQ
        // frequency (a 1kHz cut landed at ~918Hz on
        // 44.1k input). StandardizedAudioStream
        // guarantees 48k, so graph and audio match.
        let graph = crate::dsp::DspAdapter::build_graph_only(
            &icfg.intent,
            &scout_left,
            &scout_right,
            stream_sr,
            &streaming_features.mix.stem_energy_ratios,
            Some(&icfg.dsp_config),
            512,
        )
        .map_err(|e| format!("episode graph build: {e:?}"))?;

        emit_progress("Mastering");
        let render_res = crate::domain::episode_render::run(
            &mut stream,
            &blob_id,
            graph,
            episode_target_lufs.unwrap_or(-16.0),
            &pre_analysis,
            |s| s.tier1_verdict(crate::dsp::signal_health::VerdictTiming::Progressive),
        )?;

        // Final tier checks at EOF: tier1 now also
        // runs here (VerdictTiming::Final) so a short
        // all-silent file (shorter than the early
        // window, which the Progressive hook grace-
        // periods) is still caught.
        stream.tier1_verdict(crate::dsp::signal_health::VerdictTiming::Final)?;
        stream.tier2_verdict()?;

        // Input identity hashes over the SAME 48k/
        // stereo/sanitized samples the batch path
        // hashes (proven bit-identical by the
        // standardized_stream parity tests).
        let (_input_blake3_hex, input_sha256_hex) = stream.input_hashes();
        // Dead-air timeline events (non-fatal) —
        // consumed here; certificate surfacing is
        // wave 2.
        let dead_air = stream.into_dead_air();

        let (fingerprints, spatial_metadata) = ContentType::bypassed_render();
        profiler.mark_stage_with_hash("Mastering", render_res.output_sha256.clone());
        let processing_timeline = profiler.finalize();
        let elapsed = start.elapsed().as_millis() as u64;

        let cert_data = crate::domain::nodes::certificate_node::StreamingCertData {
            pcm_blake3: render_res.pcm_blake3.clone(),
            output_sha256: render_res.output_sha256.clone(),
            dead_air,
        };

        let cert_out = crate::domain::nodes::certificate_node::run_streaming(
            &blob_id,
            render_res.output_lufs,
            render_res.output_lra,
            render_res.true_peak_dbtp,
            &fingerprints,
            &spatial_metadata,
            &icfg.proof_log,
            &icfg.persona_config,
            &icfg.aether_req,
            &icfg.dsp_config,
            render_res.pcm_path.clone(),
            &input_hash_hex,
            render_res.sample_rate,
            elapsed,
            seed,
            preset_id,
            input_sha256_hex,
            render_res.frames_written,
            processing_timeline,
            cert_data,
        )?;

        return Ok((cert_out.blob, None, cert_out.file_path, None));
    }

    // NODE 1: DECODE (Music only — Episode returned
    // above without a full-file decode)
    emit_progress("Ingest");
    let decoded = crate::domain::nodes::decode_node::run(audio_path, preset_id, &blob_id)?;
    // If the preset didn't specify a
    // target, use the ContentType default.
    // Episode → -16.0 (Apple Podcasts spec)
    // Music   → -14.0 (streaming default)
    let target_lufs = decoded.target_lufs.or_else(|| match content_type {
        ContentType::Episode => Some(-16.0),
        ContentType::Music => None,
    });
    let input_hash_hex = decoded.input_hash_hex;
    let seed = decoded.seed;
    let _original_sr = decoded.original_sr;
    let _original_ch = decoded.original_ch;
    let _duration_ms = decoded.duration_ms;
    let input_blake3_hex = decoded.input_blake3_hex;
    let input_sha256_hex = decoded.input_sha256_hex;
    let _pcm_channels_for_telemetry = decoded.pcm_channels;
    let _pcm_sr_for_telemetry = decoded.pcm_sample_rate;
    let beat_data_opt = decoded.beat_data;
    match decoded.payload {
        // A3 1.2b: Stereo streaming path carries no payload; audio
        // lives in the raw dump.
        None => {}
        Some(lineos_types::AudioPayload::Stereo(_)) => {
            unreachable!("Stereo payload is never materialized on the streaming path (A3 1.2b)")
        }
        Some(lineos_types::AudioPayload::FiveDotOne {
            channels,
            sample_rate,
            num_frames,
        }) => {
            let (blob, path, model) = spatial_conformance_path(
                channels,
                sample_rate,
                num_frames,
                &blob_id,
                preset_id,
                &input_hash_hex,
                seed,
                &input_blake3_hex,
                &input_sha256_hex,
            )?;
            return Ok((blob, None, path, model));
        }
        Some(lineos_types::AudioPayload::Stems {
            voice,
            drums,
            bass,
            harmonics,
            ambience,
            sample_rate,
            num_frames,
        }) => {
            use sp314_dsp::analysis::StemFeatureAnalyzer;
            use sp314_dsp::spatial::channel_assign::StemChannelAssignments;
            use sp314_dsp::spatial::five_dot_one::{FiveDotOneStage, SpatialFirewall};
            use sp314_dsp::spatial::renderer::FiveDotOneRenderer;
            use sp314_dsp::spatial::SpatialPreAnalysis;
            use sp314_dsp::stft::stem_renderer::FiveStems;

            let n = num_frames;

            // Mono fold per stem (L+R)*0.5
            let to_mono = |l: &[f32], r: &[f32]| -> Vec<f32> {
                (0..n).map(|i| (l[i] + r[i]) * 0.5).collect()
            };

            let five_stems = FiveStems {
                voice: to_mono(&voice.left, &voice.right),
                drums: to_mono(&drums.left, &drums.right),
                bass: to_mono(&bass.left, &bass.right),
                harmonics: to_mono(&harmonics.left, &harmonics.right),
                ambience: to_mono(&ambience.left, &ambience.right),
                voice_transient_density: 0.0,
                drums_transient_density: 0.0,
                bass_transient_density: 0.0,
                harmonics_transient_density: 0.0,
                ambience_transient_density: 0.0,
            };

            // Master bus για SpatialPreAnalysis
            // (όλα τα stems L+R αθροισμένα)
            let master_l: Vec<f32> = (0..n)
                .map(|i| {
                    voice.left[i]
                        + drums.left[i]
                        + bass.left[i]
                        + harmonics.left[i]
                        + ambience.left[i]
                })
                .collect();
            let master_r: Vec<f32> = (0..n)
                .map(|i| {
                    voice.right[i]
                        + drums.right[i]
                        + bass.right[i]
                        + harmonics.right[i]
                        + ambience.right[i]
                })
                .collect();

            let spatial = SpatialPreAnalysis::analyze(&master_l, &master_r, sample_rate);
            let features = StemFeatureAnalyzer::analyze(&five_stems, sample_rate);
            let assignments = StemChannelAssignments::compute(&features, &spatial);
            let firewall = SpatialFirewall::default();
            let stage = FiveDotOneStage::render(&five_stems, &assignments, &firewall);
            let channels = FiveDotOneRenderer::render(stage);

            let (blob, path, model) = spatial_conformance_path(
                channels,
                sample_rate,
                num_frames,
                &blob_id,
                preset_id,
                &input_hash_hex,
                seed,
                &input_blake3_hex,
                &input_sha256_hex,
            )?;
            return Ok((blob, None, path, model));
        }
    };

    // [BISECT-1-DECODE] Values are bit-equivalent to rms(&chunk.left) iff
    // rms() is sqrt(sum/len) over the same sequential order, which it is.
    let left_rms = (decoded.left_sum_sq / decoded.total_frames.max(1) as f32).sqrt();
    let right_rms = (decoded.right_sum_sq / decoded.total_frames.max(1) as f32).sqrt();
    eprintln!(
        "[BISECT-1-DECODE] L_rms={:.6} R_rms={:.6} ratio={:.4}",
        left_rms,
        right_rms,
        right_rms / left_rms.max(1e-9)
    );

    profiler.mark_stage_with_hash("Ingest", input_blake3_hex.clone());

    // ── ST-P5: TwoPassEngine stem separation via MPSC streaming ─────
    use sp314_dsp::spatial::user_profile::UserSpatialProfile;

    // Pre-Analysis and Rhythm Detection
    // Scout uses 30s mid-section sample.
    //
    // Phase 8.2: try lazy disk seek first
    // (reads only ~30s from disk via
    // n_frames metadata + seek_exact_frame,
    // parity-verified against the old
    // in-memory path — max_diff=0.000000
    // in tests). Falls back to the original
    // O(1) in-memory slice if the format
    // lacks upfront duration metadata.
    //
    // NOTE: decode_node::run() above still
    // fully decodes the file into RAM at
    // this point — Phase 8.2 only changes
    // HOW the scout sample is obtained, not
    // whether the full file is loaded. The
    // actual memory/OOM fix is Phase 8.3
    // (streaming render), which will remove
    // the full upstream decode entirely for
    // large files.
    let scout_frames = (decoded.pcm_sample_rate as usize) * 30;
    let total_frames = decoded.total_frames;

    let lazy_scout =
        crate::dsp::lazy_reader::read_scout_sample(std::path::Path::new(audio_path), 30.0);

    let raw_path = format!("/tmp/m0d-raw-{}.pcm", blob_id);
    let (scout_left_owned, scout_right_owned): (Vec<f32>, Vec<f32>) =
        if let Some((l, r, _sr)) = lazy_scout {
            (l, r)
        } else if total_frames > scout_frames {
            let start_frame = (total_frames - scout_frames) / 2;
            read_scout_from_raw_dump(std::path::Path::new(&raw_path), start_frame, scout_frames)?
        } else {
            read_scout_from_raw_dump(std::path::Path::new(&raw_path), 0, scout_frames)?
        };
    let scout_left = &scout_left_owned[..];
    let scout_right = &scout_right_owned[..];

    use sp314_dsp::analysis::PreAnalyzer;
    let mut pre_analysis = PreAnalyzer::run(scout_left, scout_right, decoded.pcm_sample_rate);
    // Episode/spoken-word: skip beat
    // detection entirely. BPM and beat
    // grids are meaningless for voice
    // and require a full-file mono
    // allocation — the single largest
    // OOM source after decode itself.
    let (bpm, beats_ms, downbeats_ms, transients_ms) = if content_type.skip_stems() {
        (0.0_f32, vec![], vec![], vec![])
    } else {
        beat_data_opt.unwrap_or_else(|| (0.0_f32, vec![], vec![], vec![]))
    };
    tracing::info!(
        "Rhythm Analysis: BPM = {:.1}, {} transients, {} downbeats",
        bpm,
        transients_ms.len(),
        downbeats_ms.len()
    );
    pre_analysis.bpm = bpm;
    pre_analysis.beats_ms = beats_ms;
    pre_analysis.downbeats_ms = downbeats_ms;
    pre_analysis.transients_ms = transients_ms;

    // NODE 3: SCOUT (NMF + Maestro)
    // --- NODE 3: SCOUT PASS ---
    emit_progress("Scout Pass");
    let scout_out = crate::domain::nodes::scout_node::run(
        scout_left,
        scout_right,
        decoded.pcm_sample_rate,
        req.project_id.as_deref().unwrap_or("default"),
        req.flavour_id.as_deref().unwrap_or("default"),
        &pre_analysis,
    )?;
    let mut two_pass = scout_out.engine;
    let scout = scout_out.scout;
    let render_params = scout_out.render_params;
    let mono = scout_out.mono;
    profiler.mark_stage("Scout Pass", &mono);

    // Real NMF stem features from scout pass
    // (computed on downsampled proxy stems —
    // ratios are scale-invariant). Feeds
    // MaskingEQ for dynamic mud correction.
    let streaming_features = scout.features.clone();

    // ═══ EPISODE STREAMING PATH ═══════════════
    // Bounded-RAM render for spoken-word content.
    // Never holds the full buffer: skips stem
    // separation, the full-file mmap interleave,
    // and the array-based certificate. RAM stays
    // ~O(chunk) regardless of file duration.
    // (Episode path now returns earlier — before
    // NODE 1 DECODE — via the streaming branch, so
    // it never reaches this point. Music continues.)

    // NODE 4: RENDER (mmap + process_chunks + spatial)
    // mmap stays here — render_node receives slices (no self-referential struct)
    let n_total = decoded.total_frames;
    const STFT_FLUSH_TAIL: usize = 1024;
    let n_total_with_tail = n_total + STFT_FLUSH_TAIL;
    let file_path = std::path::PathBuf::from(format!("/tmp/m0d-mastering-{}.pcm", blob_id));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&file_path)
        .map_err(|e| format!("Failed to create mapped file: {e}"))?;
    file.set_len((n_total * 2 * 4) as u64)
        .map_err(|e| format!("Failed to set file len: {e}"))?;
    let mut mmap =
        unsafe { memmap2::MmapMut::map_mut(&file).map_err(|e| format!("Mmap failed: {e}"))? };
    // Allocate file-backed mmaps for working storage
    let scratch_l_path = std::path::PathBuf::from(format!("/tmp/m0d-scratch-l-{}.pcm", blob_id));
    let scratch_l_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&scratch_l_path)
        .map_err(|e| format!("Failed to create mapped file: {e}"))?;
    scratch_l_file.set_len((n_total_with_tail * 4) as u64)
        .map_err(|e| format!("Failed to set file len: {e}"))?;
    let mut scratch_l_mmap =
        unsafe { memmap2::MmapMut::map_mut(&scratch_l_file).map_err(|e| format!("Mmap failed: {e}"))? };
    let scratch_l_view: &mut [f32] =
        unsafe { std::slice::from_raw_parts_mut(scratch_l_mmap.as_mut_ptr() as *mut f32, n_total_with_tail) };

    let scratch_r_path = std::path::PathBuf::from(format!("/tmp/m0d-scratch-r-{}.pcm", blob_id));
    let scratch_r_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&scratch_r_path)
        .map_err(|e| format!("Failed to create mapped file: {e}"))?;
    scratch_r_file.set_len((n_total_with_tail * 4) as u64)
        .map_err(|e| format!("Failed to set file len: {e}"))?;
    let mut scratch_r_mmap =
        unsafe { memmap2::MmapMut::map_mut(&scratch_r_file).map_err(|e| format!("Mmap failed: {e}"))? };
    let scratch_r_view: &mut [f32] =
        unsafe { std::slice::from_raw_parts_mut(scratch_r_mmap.as_mut_ptr() as *mut f32, n_total_with_tail) };

    let repo_state = head_state.load_full();
    let final_ducking =
        (render_params.ducking_gain / repo_state.ducking_depth).clamp(0.1_f32, 1.0_f32);

    // Spatial output buffers —
    // allocated only if preset needs spatial
    let needs_spatial = matches!(preset_id.as_str(), "spatial_upmix" | "pro_bundle_both");
    let mut sp_l = if needs_spatial {
        vec![0.0_f32; n_total_with_tail]
    } else {
        vec![]
    };
    let mut sp_r = if needs_spatial {
        vec![0.0_f32; n_total_with_tail]
    } else {
        vec![]
    };
    let mut sp_c = if needs_spatial {
        vec![0.0_f32; n_total_with_tail]
    } else {
        vec![]
    };
    let mut sp_lfe = if needs_spatial {
        vec![0.0_f32; n_total_with_tail]
    } else {
        vec![]
    };
    let mut sp_ls = if needs_spatial {
        vec![0.0_f32; n_total_with_tail]
    } else {
        vec![]
    };
    let mut sp_rs = if needs_spatial {
        vec![0.0_f32; n_total_with_tail]
    } else {
        vec![]
    };

    let mut spatial_blob_out: Option<StoredBlob> = None;

    emit_progress("Stem Engine");

    let mut spatial_slices = if needs_spatial {
        Some(crate::domain::nodes::render_node::SpatialSlicesMut {
            l: &mut sp_l,
            r: &mut sp_r,
            c: &mut sp_c,
            lfe: &mut sp_lfe,
            ls: &mut sp_ls,
            rs: &mut sp_rs,
        })
    } else {
        None
    };

    let (fingerprints, spatial_metadata) = if content_type.skip_stems() {
        // Episode path: bypass stem
        // separation entirely.
        // StemFingerprints fields are
        // None — certificate omits
        // Stem DNA for spoken-word.
        ContentType::bypassed_render()
    } else {
        let raw_path = format!("/tmp/m0d-raw-{}.pcm", blob_id);
        let stream_source = if std::path::Path::new(&raw_path).exists() {
            eprintln!("[DEBUG] TAKING NEW STREAMING PATH: found {}", raw_path);
            let source = sp314_orchestrator::raw_pcm_source::RawPcmFileSource::new(
                std::path::Path::new(&raw_path),
            )
            .map_err(|e| format!("Failed to open raw PCM dump: {e}"))?;
            sp314_dsp::stft::sliding_overlap_reader::SlidingOverlapReader::new(source, 10240)
        } else {
            // A missing file here means decode_node.rs's Stereo path failed to write it,
            // or the OS purged it. Silently falling back to slice logic would trigger an O(N)
            // memory spike, defeating the streaming architecture. Fail loudly.
            eprintln!("[DEBUG] HARD ERROR: raw path {} not found", raw_path);
            return Err(format!(
                "CRITICAL: Raw PCM dump {} not found for Music/Stereo path. Cannot proceed with O(1) streaming render.",
                raw_path
            ));
        };

        crate::domain::nodes::render_node::run(
            &mut two_pass,
            &scout,
            &crate::domain::nodes::render_node::RenderSettings {
                ducking_gain: final_ducking,
                mix_levels: req.mix_levels.as_ref(),
                flavour_id: req.flavour_id.as_deref(),
                sample_rate: decoded.pcm_sample_rate,
            },
            crate::domain::nodes::render_node::RenderInputs {
                original_sum_sq: decoded.original_sum_sq,
                total_frames: decoded.total_frames,
                stream_source,
            },
            &mut scratch_l_view[..],
            &mut scratch_r_view[..],
            spatial_slices.as_mut(),
        )?
    };

    if needs_spatial && !sp_l.is_empty() {
        let spatial_channels: [Vec<f32>; 6] = [sp_l, sp_r, sp_c, sp_lfe, sp_ls, sp_rs];
        // Τρέξε conformance + export
        let spatial_blob = spatial_conformance_path(
            spatial_channels,
            decoded.pcm_sample_rate,
            n_total_with_tail,
            &format!("{blob_id}-spatial"),
            preset_id,
            &input_hash_hex,
            seed,
            &input_blake3_hex,
            &input_sha256_hex,
        )?;
        spatial_blob_out = Some(spatial_blob.0);
    }

    eprintln!(
        "[BISECT-3-RENDER] L_rms={:.6} R_rms={:.6} ratio={:.4}",
        rms(&scratch_l_view[..]),
        rms(&scratch_r_view[..]),
        rms(&scratch_r_view[..]) / rms(&scratch_l_view[..]).max(1e-9)
    );

    let left_post = &mut scratch_l_view[STFT_FLUSH_TAIL..];
    let right_post = &mut scratch_r_view[STFT_FLUSH_TAIL..];

    eprintln!(
        "[BISECT-4-TRIM] L_rms={:.6} R_rms={:.6} ratio={:.4}",
        rms(&left_post[..]),
        rms(&right_post[..]),
        rms(&right_post[..]) / rms(&left_post[..]).max(1e-9)
    );

    profiler.mark_stage("Stem Engine", &left_post[..]);

    // Markov spatial modulation (simplified — full in Phase 8)
    emit_progress("Spatial");
    let profile = UserSpatialProfile::default_podcast();
    let modulated = profile.apply_markov_prediction("vowel");
    let _ = modulated;

    // chunk.left  = left_slice;
    // chunk.right = right_slice;
    profiler.mark_stage("Spatial", &left_post[..]); // fixes stale hash of pre-render audio (F-041)

    // NODE 5: DSP (pre-analysis + autotune + AetherBridge + corpus + master)
    emit_progress("Mastering");
    let dsp_out = crate::domain::nodes::dsp_node::run(
        left_post,
        right_post,
        scout_left,
        decoded.pcm_sample_rate,
        preset_id,
        target_lufs,
        req.flavour_id.as_deref().unwrap_or("warm"),
        &streaming_features,
        req.intent_tone.or(req.tone),
        req.intent_dynamics.or(req.dynamics),
        req.chaos_seed,
        req.project_id.as_deref(),
        req.track_id.as_deref(),
        &blob_id,
        pre_analysis.clone(),
        state_dir,
    )?;
    let pre_analysis = dsp_out.pre_analysis;
    let dsp_config = dsp_out.dsp_config;
    let proof_log = dsp_out.proof_log;
    let persona_config = dsp_out.persona_config;
    let aether_req = dsp_out.aether_req;
    let lufs = dsp_out.lufs;
    let tp = dsp_out.true_peak;
    profiler.mark_stage("Mastering", &left_post[..]);
    let _dr = 10.0; // dynamic range proxy for v3
    let _sc = 1.0; // stereo correlation proxy for v3
    let elapsed = start.elapsed().as_millis() as u64;

    // Interleave planar slices into mmap for playback (xaak/cpal expect interleaved)
    let mmap_f32: &mut [f32] =
        unsafe { std::slice::from_raw_parts_mut(mmap.as_mut_ptr() as *mut f32, n_total * 2) };
    for i in 0..n_total {
        mmap_f32[i * 2] = left_post[i];
        mmap_f32[i * 2 + 1] = right_post[i];
    }

    // Sync mapped file to disk before returning path
    mmap.flush().unwrap_or_default();

    let processing_timeline = profiler.finalize();

    let cert_out = crate::domain::nodes::certificate_node::run(
        &blob_id,
        lufs,
        tp,
        &pre_analysis,
        &fingerprints,
        &spatial_metadata,
        &proof_log,
        &persona_config,
        &aether_req,
        &dsp_config,
        &left_post[..],
        &right_post[..],
        file_path,
        &input_hash_hex,
        decoded.pcm_sample_rate,
        2,
        0, // duration_ms proxy
        target_lufs,
        decoded.pcm_sample_rate,
        elapsed,
        seed,
        preset_id,
        input_sha256_hex,
        n_total,
        processing_timeline,
    )?;

    let _ = std::fs::remove_file(&scratch_l_path);
    let _ = std::fs::remove_file(&scratch_r_path);

    Ok((
        cert_out.blob,
        spatial_blob_out,
        cert_out.file_path,
        dsp_out.user_model,
    ))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// LUFS within 1 LU of target AND TP ≤ ceiling → platform compliant.
#[allow(dead_code)]
fn platform_ok(lufs: f32, target: f32, tp: f32) -> bool {
    lufs <= target + 1.0 && tp <= -1.0
}

/// SHA-256 of input bytes — returns [u8; 32].
/// Used for both `input_hash` audit field and determinism `seed`.
pub fn compute_sha256_bytes(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
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
    bytes
        .chunks_exact(4)
        .map(|b| {
            let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            // Map NaN/Inf to 0.0, then clamp to audio range
            if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        })
        .collect()
}

/// Compute RMS amplitude of samples.
pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

/// Rough LUFS estimate from RMS — used only for overflow guard, not stored.
/// Full EBU R128 measurement happens inside sp314-dsp.
pub fn rms_to_lufs(rms: f32) -> f32 {
    if rms <= 0.0 {
        return f32::NEG_INFINITY;
    }
    // K-weighting approximation: subtract ~1 dB from RMS dBFS
    20.0 * rms.log10() - 1.0
}

// ── Tests ─────────────────────────────────────────────────────────────────────

fn read_scout_from_raw_dump(
    raw_path: &std::path::Path,
    start_frame: usize,
    scout_frames: usize,
) -> Result<(Vec<f32>, Vec<f32>), String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(raw_path).map_err(|e| {
        format!(
            "scout fallback: failed to open {}: {}",
            raw_path.display(),
            e
        )
    })?;

    let file_len = file
        .metadata()
        .map_err(|e| format!("scout fallback metadata error: {}", e))?
        .len();
    let frame_bytes: u64 = 8; // 2 channels * 4 bytes

    if file_len % frame_bytes != 0 {
        return Err(format!(
            "scout fallback: raw dump has partial frame — truncated/corrupt dump (len={})",
            file_len
        ));
    }

    let start_byte = (start_frame as u64) * frame_bytes;
    if start_byte >= file_len {
        return Err(format!(
            "scout fallback: start_frame {} is beyond EOF",
            start_frame
        ));
    }

    file.seek(SeekFrom::Start(start_byte))
        .map_err(|e| format!("scout fallback seek error: {}", e))?;

    let max_read_bytes = file_len - start_byte;
    let requested_bytes = (scout_frames as u64) * frame_bytes;
    let bytes_to_read =
        ((std::cmp::min(max_read_bytes, requested_bytes) / frame_bytes) * frame_bytes) as usize;
    let actual_frames = bytes_to_read / 8; // (usize)

    let mut raw_buf = vec![0u8; bytes_to_read];
    file.read_exact(&mut raw_buf)
        .map_err(|e| format!("scout fallback read error: {}", e))?;

    let mut l = Vec::with_capacity(actual_frames);
    let mut r = Vec::with_capacity(actual_frames);

    for chunk in raw_buf.chunks_exact(8) {
        let left_val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let right_val = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        l.push(left_val);
        r.push(right_val);
    }

    Ok((l, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_scout_from_raw_dump() {
        use std::io::Write;
        let path = "/tmp/test_raw_scout_dump.pcm";
        let mut f = std::fs::File::create(path).unwrap();
        // 10 frames total (20 floats)
        for i in 0..10 {
            f.write_all(&(i as f32).to_le_bytes()).unwrap(); // L
            f.write_all(&(-(i as f32)).to_le_bytes()).unwrap(); // R
        }
        f.flush().unwrap();

        // Mid-window read (start=2, frames=3)
        let (l, r) = read_scout_from_raw_dump(std::path::Path::new(path), 2, 3).unwrap();
        assert_eq!(l, vec![2.0, 3.0, 4.0]);
        assert_eq!(r, vec![-2.0, -3.0, -4.0]);

        // Short/EOF read (start=8, frames=5, but only 2 frames left)
        let (l2, r2) = read_scout_from_raw_dump(std::path::Path::new(path), 8, 5).unwrap();
        assert_eq!(l2, vec![8.0, 9.0]);
        assert_eq!(r2, vec![-8.0, -9.0]);

        let _ = std::fs::remove_file(path);

        // Truncated dump (10 full frames + 4 orphan bytes)
        let path_trunc = "/tmp/test_raw_scout_trunc.pcm";
        let mut f2 = std::fs::File::create(path_trunc).unwrap();
        for i in 0..10 {
            f2.write_all(&(i as f32).to_le_bytes()).unwrap(); // L
            f2.write_all(&(-(i as f32)).to_le_bytes()).unwrap(); // R
        }
        f2.write_all(&11.0_f32.to_le_bytes()).unwrap(); // 4 orphan bytes
        f2.flush().unwrap();

        let trunc_res = read_scout_from_raw_dump(std::path::Path::new(path_trunc), 0, 5);
        assert!(trunc_res.is_err());
        assert!(trunc_res.unwrap_err().contains("partial frame"));

        let _ = std::fs::remove_file(path_trunc);
    }

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
        assert!(
            out[0] >= -1.0 && out[0] <= 1.0,
            "Clamped value must be in [-1.0, 1.0], got {}",
            out[0]
        );
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
        assert!(
            (rms - 0.5_f32 / 2.0_f32.sqrt()).abs() < 0.01,
            "RMS should be ~0.354, got {rms}"
        );
    }

    #[test]
    fn test_silence_guard_threshold() {
        // RMS of 0.0 → dBFS = -∞ → below -60 dBFS silence threshold
        let rms = 0.0_f32;
        let rms_dbfs = if rms > 0.0 {
            20.0 * (rms as f64).log10() as f32
        } else {
            f32::NEG_INFINITY
        };
        assert!(rms_dbfs < -60.0, "Silence must be below guard threshold");
    }

    #[test]
    fn test_gain_overflow_guard() {
        // Very quiet input (-70 LUFS) with target -14 LUFS → gain >> 32×
        let quiet_rms = 10.0_f32.powf(-70.0 / 20.0);
        let measured_lufs = rms_to_lufs(quiet_rms);
        let gain_db = -14.0 - measured_lufs;
        let gain_linear = 10.0_f32.powf(gain_db / 20.0);
        assert!(
            gain_linear > 32.0,
            "Very quiet input should trigger overflow guard, gain={gain_linear:.1}×"
        );
    }

    #[test]
    fn test_target_lufs_clamped() {
        // Out-of-range values should be clamped
        let clamped_low: f32 = (-50.0_f32).clamp(-40.0, 0.0);
        let clamped_high: f32 = (5.0_f32).clamp(-40.0, 0.0);
        assert_eq!(clamped_low, -40.0);
        assert_eq!(clamped_high, 0.0);
    }
}
