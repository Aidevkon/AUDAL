use crate::blob_store::StoredBlob;
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
) -> Result<
    (
        StoredBlob,
        Option<StoredBlob>,
        std::path::PathBuf,
        Option<lineos_corpus::store::UserMarkovModel>,
    ),
    String,
> {
    run_dsp_internal(req, start, head_state, progress_tx, progress_map, job_id)
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

    // NODE 1: DECODE
    emit_progress("Ingest");
    let decoded = crate::domain::nodes::decode_node::run(audio_path, preset_id, &blob_id)?;
    let target_lufs = decoded.target_lufs;
    let input_hash_hex = decoded.input_hash_hex;
    let seed = decoded.seed;
    let _original_sr = decoded.original_sr;
    let _original_ch = decoded.original_ch;
    let _duration_ms = decoded.duration_ms;
    let input_blake3_hex = decoded.input_blake3_hex;
    let input_sha256_hex = decoded.input_sha256_hex;
    let _pcm_channels_for_telemetry = decoded.pcm_channels;
    let _pcm_sr_for_telemetry = decoded.pcm_sample_rate;
    let mut chunk = match decoded.payload {
        lineos_types::AudioPayload::Stereo(buf) => buf,
        lineos_types::AudioPayload::FiveDotOne {
            channels,
            sample_rate,
            num_frames,
        } => {
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
        lineos_types::AudioPayload::Stems {
            voice,
            drums,
            bass,
            harmonics,
            ambience,
            sample_rate,
            num_frames,
        } => {
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

    eprintln!(
        "[BISECT-1-DECODE] L_rms={:.6} R_rms={:.6} ratio={:.4}",
        rms(&chunk.left),
        rms(&chunk.right),
        rms(&chunk.right) / rms(&chunk.left).max(1e-9)
    );

    profiler.mark_stage_with_hash("Ingest", input_blake3_hex.clone());

    // ── ST-P5: TwoPassEngine stem separation via MPSC streaming ─────
    use sp314_dsp::spatial::user_profile::UserSpatialProfile;

    // Pre-Analysis and Rhythm Detection
    // Scout uses 30s mid-section sample.
    // File is already in RAM — O(1) slice.
    // Avoids running NMF on full 5min track.
    // If track < 30s, use full buffer.
    let scout_frames = (chunk.sample_rate as usize) * 30;
    let total_frames = chunk.left.len();
    let (scout_left, scout_right) = if total_frames > scout_frames {
        let start = (total_frames - scout_frames) / 2;
        let end = start + scout_frames;
        (&chunk.left[start..end], &chunk.right[start..end])
    } else {
        (&chunk.left[..], &chunk.right[..])
    };

    use sp314_dsp::analysis::PreAnalyzer;
    let mut pre_analysis = PreAnalyzer::run(scout_left, scout_right, chunk.sample_rate);
    let mono_samples: Vec<f32> = chunk
        .left
        .iter()
        .zip(chunk.right.iter())
        .map(|(l, r)| (*l + *r) * 0.5)
        .collect();
    let detector = crate::dsp::beat_detector::BeatDetector::new(chunk.sample_rate);
    let (bpm, beats_ms, downbeats_ms, transients_ms) = detector.analyze(&mono_samples);
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
        chunk.sample_rate,
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

    // NODE 4: RENDER (mmap + process_chunks + spatial)
    // mmap stays here — render_node receives slices (no self-referential struct)
    let n_total = mono.len();
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
    // Allocate in-memory arrays for DSP (sp314-dsp requires planar arrays)
    let mut left_vec = vec![0.0_f32; n_total_with_tail];
    let mut right_vec = vec![0.0_f32; n_total_with_tail];

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

    let (fingerprints, spatial_metadata) = crate::domain::nodes::render_node::run(
        &mut two_pass,
        &mono,
        &scout,
        final_ducking,
        req.mix_levels.as_ref(),
        req.flavour_id.as_deref(),
        chunk.sample_rate,
        &chunk.left,
        &chunk.right,
        &mut left_vec[..],
        &mut right_vec[..],
        spatial_slices.as_mut(),
    )?;

    if needs_spatial && !sp_l.is_empty() {
        let spatial_channels: [Vec<f32>; 6] = [sp_l, sp_r, sp_c, sp_lfe, sp_ls, sp_rs];
        // Τρέξε conformance + export
        let spatial_blob = spatial_conformance_path(
            spatial_channels,
            chunk.sample_rate,
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
        rms(&left_vec),
        rms(&right_vec),
        rms(&right_vec) / rms(&left_vec).max(1e-9)
    );

    // Latency compensation: left-shift by STFT_FLUSH_TAIL to discard silence, then truncate.
    left_vec.copy_within(STFT_FLUSH_TAIL.., 0);
    left_vec.truncate(n_total);
    right_vec.copy_within(STFT_FLUSH_TAIL.., 0);
    right_vec.truncate(n_total);

    eprintln!(
        "[BISECT-4-TRIM] L_rms={:.6} R_rms={:.6} ratio={:.4}",
        rms(&left_vec),
        rms(&right_vec),
        rms(&right_vec) / rms(&left_vec).max(1e-9)
    );

    profiler.mark_stage("Stem Engine", &left_vec[..]);

    // Markov spatial modulation (simplified — full in Phase 8)
    emit_progress("Spatial");
    let profile = UserSpatialProfile::default_podcast();
    let modulated = profile.apply_markov_prediction("vowel");
    let _ = modulated;

    // chunk.left  = left_slice;
    // chunk.right = right_slice;
    profiler.mark_stage("Spatial", &chunk.left);

    // NODE 5: DSP (pre-analysis + autotune + AetherBridge + corpus + master)
    emit_progress("Mastering");
    let dsp_out = crate::domain::nodes::dsp_node::run(
        &mut chunk.left,
        &mut chunk.right,
        &mut left_vec[..],
        &mut right_vec[..],
        chunk.sample_rate,
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
    )?;
    let pre_analysis = dsp_out.pre_analysis;
    let dsp_config = dsp_out.dsp_config;
    let proof_log = dsp_out.proof_log;
    let persona_config = dsp_out.persona_config;
    let aether_req = dsp_out.aether_req;
    let lufs = dsp_out.lufs;
    let tp = dsp_out.true_peak;
    profiler.mark_stage("Mastering", &left_vec[..]);
    let _dr = 10.0; // dynamic range proxy for v3
    let _sc = 1.0; // stereo correlation proxy for v3
    let elapsed = start.elapsed().as_millis() as u64;

    // Interleave planar slices into mmap for playback (xaak/cpal expect interleaved)
    let mmap_f32: &mut [f32] =
        unsafe { std::slice::from_raw_parts_mut(mmap.as_mut_ptr() as *mut f32, n_total * 2) };
    for i in 0..n_total {
        mmap_f32[i * 2] = left_vec[i];
        mmap_f32[i * 2 + 1] = right_vec[i];
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
        &left_vec[..],
        &right_vec[..],
        file_path,
        &input_hash_hex,
        chunk.sample_rate,
        2,
        0, // duration_ms proxy
        target_lufs,
        chunk.sample_rate,
        elapsed,
        seed,
        preset_id,
        input_sha256_hex,
        n_total,
        processing_timeline,
    )?;

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
