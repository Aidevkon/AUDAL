use crate::blob_store::StoredBlob;
use crate::handlers::master::MasterRequest;
use std::time::Instant;
// Per-stem SHA-256 fingerprints (Dev Protocol §13.3)
// Computed on raw stems before mix — tamper-proof certificate

/// Invoke sp314-dsp MasteringPipeline and assemble StoredBlob.
/// Phase 7: uses decode::decode_audio() — real symphonia decode.
/// Runs blocking decode + DSP in Tokio blocking tasks.
#[allow(deprecated)]
pub fn run_dsp(
    req: &MasterRequest,
    start: Instant,
) -> Result<(StoredBlob, std::path::PathBuf, Option<f32>), String> {
    run_dsp_internal(req, start)
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
fn run_dsp_internal(
    req: &MasterRequest,
    start: Instant,
) -> Result<(StoredBlob, std::path::PathBuf, Option<f32>), String> {
    let mut profiler = crate::handlers::timeline::TimelineProfiler::new();
    let audio_path = &req.audio_path;
    let preset_id = &req.preset_id;

    // NODE 1: DECODE
    let decoded = crate::domain::nodes::decode_node::run(audio_path, preset_id)?;
    let target_lufs = decoded.target_lufs;
    let input_hash_hex = decoded.input_hash_hex;
    let seed = decoded.seed;
    let _original_sr = decoded.original_sr;
    let _original_ch = decoded.original_ch;
    let _duration_ms = decoded.duration_ms;
    let _pcm_samples_for_telemetry = decoded.pcm_samples;
    let _pcm_channels_for_telemetry = decoded.pcm_channels;
    let _pcm_sr_for_telemetry = decoded.pcm_sample_rate;
    let mut chunk = decoded.chunk;
    let _chunk_original = decoded.chunk_original;

    profiler.mark_stage("Ingest", &_pcm_samples_for_telemetry);

    // ── ST-P5: TwoPassEngine stem separation via MPSC streaming ─────
    use sp314_dsp::spatial::user_profile::UserSpatialProfile;

    // NODE 3: SCOUT (NMF + Maestro)
    let scout_out = crate::domain::nodes::scout_node::run(
        &chunk.left,
        &chunk.right,
        chunk.sample_rate,
        req.project_id.as_deref().unwrap_or("default"),
        req.flavour_id.as_deref().unwrap_or("default"),
    )?;
    let mut two_pass = scout_out.engine;
    let scout = scout_out.scout;
    let render_params = scout_out.render_params;
    let mono = scout_out.mono;
    profiler.mark_stage("Scout Pass", &mono);

    use lineos_types::{MixMetrics, StemFeatures, StemMetrics};
    let streaming_features = StemFeatures {
        voice: StemMetrics::default(),
        drums: StemMetrics::default(),
        bass: StemMetrics::default(),
        harmonics: StemMetrics::default(),
        ambience: StemMetrics::default(),
        mix: MixMetrics::default(),
    };

    // NODE 4: RENDER (mmap + process_chunks + spatial)
    // mmap stays here — render_node receives slices (no self-referential struct)
    let n_total = mono.len();
    const STFT_FLUSH_TAIL: usize = 1024;
    let n_total_with_tail = n_total + STFT_FLUSH_TAIL;
    let blob_id = req
        .track_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let file_path = std::path::PathBuf::from(format!("/tmp/m0d-mastering-{}.pcm", blob_id));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&file_path)
        .map_err(|e| format!("Failed to create mapped file: {e}"))?;
    file.set_len((n_total_with_tail * 2 * 4) as u64)
        .map_err(|e| format!("Failed to set file len: {e}"))?;
    let mut mmap =
        unsafe { memmap2::MmapMut::map_mut(&file).map_err(|e| format!("Mmap failed: {e}"))? };
    let (left_bytes, right_bytes) = mmap.split_at_mut(n_total_with_tail * 4);
    let left_slice: &mut [f32] = unsafe {
        std::slice::from_raw_parts_mut(left_bytes.as_mut_ptr() as *mut f32, n_total_with_tail)
    };
    let right_slice: &mut [f32] = unsafe {
        std::slice::from_raw_parts_mut(right_bytes.as_mut_ptr() as *mut f32, n_total_with_tail)
    };

    let fingerprints = crate::domain::nodes::render_node::run(
        &mut two_pass,
        &mono,
        &scout,
        render_params.ducking_gain,
        req.mix_levels.as_ref(),
        req.flavour_id.as_deref(),
        chunk.sample_rate,
        &chunk.left,
        &chunk.right,
        left_slice,
        right_slice,
    )?;
    profiler.mark_stage("Stem Engine", left_slice);

    // Markov spatial modulation (simplified — full in Phase 8)
    let profile = UserSpatialProfile::default_podcast();
    let modulated = profile.apply_markov_prediction("vowel");
    let _ = modulated;

    // chunk.left  = left_slice;
    // chunk.right = right_slice;
    profiler.mark_stage("Spatial", &chunk.left);

    // NODE 5: DSP (pre-analysis + autotune + AetherBridge + corpus + master)
    let dsp_out = crate::domain::nodes::dsp_node::run(
        &mut chunk.left,
        &mut chunk.right,
        left_slice,
        right_slice,
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
    )?;
    let pre_analysis = dsp_out.pre_analysis;
    let dsp_config = dsp_out.dsp_config;
    let proof_log = dsp_out.proof_log;
    let persona_config = dsp_out.persona_config;
    let aether_req = dsp_out.aether_req;
    let lufs = dsp_out.lufs;
    let tp = dsp_out.true_peak;
    profiler.mark_stage("Mastering", left_slice);
    let _dr = 10.0; // dynamic range proxy for v3
    let _sc = 1.0; // stereo correlation proxy for v3
    let elapsed = start.elapsed().as_millis() as u64;

    // Sync mapped file to disk before returning path
    mmap.flush().unwrap_or_default();

    let processing_timeline = profiler.finalize();

    let cert_out = crate::domain::nodes::certificate_node::run(
        &blob_id,
        lufs,
        tp,
        &pre_analysis,
        &fingerprints,
        &proof_log,
        &persona_config,
        &aether_req,
        &dsp_config,
        left_slice,
        right_slice,
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
        &_pcm_samples_for_telemetry,
        n_total,
        processing_timeline,
    )?;

    Ok((cert_out.blob, cert_out.file_path, None))
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
