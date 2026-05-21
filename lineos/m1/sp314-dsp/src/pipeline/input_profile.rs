//! Input profile detection — sp314-dsp v2.9 §Input Profile Detection.
//!
//! Authority: sp314-dsp-v2-spec.md v2.9 §Input Profile Detection
//! Constitutional rules:
//!   - libm only — no std::f32 methods in pipeline
//!   - No f64 upcast — f32 throughout (determinism contract)
//!   - let _ = is FORBIDDEN (silent failure)
//!   - All thresholds are compile-time constants
//!
//! # Routing Contract
//!
//! Per spec §Input Profile Detection:
//!   `Silence`      → early exit (pipeline caller must handle)
//!   `DCOnly`       → `MasteringError::Input` (pipeline caller must reject)
//!   All others     → continue (detect and store only, no routing change)
//!
//! This module is responsible for **detection only**. Routing decisions
//! are the pipeline entry point's responsibility, not this function's.

use super::math::kahan_mean_square;

// ── Compile-time thresholds (§Key Invariants — all thresholds are constants) ──

/// RMS threshold for silence classification: -60 dBFS.
/// Samples whose RMS mean_square falls below this (linear²) value
/// are classified as Silence.
/// -60 dBFS → linear = 10^(-60/20) = 0.001 → mean_square = 1e-6.
const SILENCE_MEAN_SQUARE_THRESHOLD: f32 = 1e-6;

/// DC bias threshold for DCOnly classification.
/// If |mean(samples)| > this fraction of full scale, the signal is
/// dominated by DC offset rather than audio content.
const DC_BIAS_THRESHOLD: f32 = 0.01;

/// Minimum mean_square for DCOnly detection.
/// Signal must also have very low AC energy to be classified as DCOnly.
/// 1e-8 linear² ≈ -80 dBFS RMS — effectively no AC component.
const DC_ONLY_MAX_AC_MEAN_SQUARE: f32 = 1e-8;

/// Clipping detection threshold: samples at or above ±1.0 are clipped.
const CLIP_THRESHOLD: f32 = 1.0;

/// LRA proxy threshold for HighDynamic classification (approved Q1).
/// max_block_lufs − min_block_lufs > 20.0 LU → HighDynamic.
const HIGH_DYNAMIC_LU_THRESHOLD: f32 = 20.0;

/// LUFS floor: blocks below this are excluded from LRA proxy range.
/// Matches the EBU R128 absolute gate (-70 LUFS) to avoid silence
/// blocks inflating the dynamic range measurement.
const LUFS_ABSOLUTE_GATE: f32 = -70.0;

/// Block size for per-block LUFS proxy: 400ms @ 48kHz × 2 channels.
/// Matches the block size used by AnalysisAccumulator (EBU R128 §3.1).
/// Stereo interleaved: 19200 frames × 2 ch = 38400 samples.
const LUFS_BLOCK_SAMPLES_STEREO: usize = 19_200 * 2; // stereo
const LUFS_BLOCK_SAMPLES_MONO: usize = 19_200;       // mono

// ─────────────────────────────────────────────────────────────────────────────

/// Classification of the input PCM signal's characteristics.
///
/// Per spec §Input Profile Detection:
/// - `Silence`      → below -60 dBFS RMS → early exit
/// - `DCOnly`       → near-zero AC audio with DC bias → `MasteringError::Input`
/// - `Clipped`      → samples at ±1.0 → continue with warning
/// - `HighDynamic`  → LRA proxy > 20 LU → continue
/// - `MonoInStereo` → identical L/R channels → continue
/// - `Normal`       → none of the above → continue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputProfile {
    Normal,
    Silence,
    Clipped,
    HighDynamic,
    DCOnly,
    MonoInStereo,
}

/// Detect the input profile of `pcm`.
///
/// `pcm` is interleaved PCM at 48kHz. `channels` is 1 (mono) or 2 (stereo).
///
/// Detection order (first match wins):
/// 1. Silence   — RMS mean_square < -60 dBFS²
/// 2. DCOnly    — low AC energy + significant DC mean
/// 3. Clipped   — any |sample| >= 1.0
/// 4. MonoInStereo — stereo input with identical L/R channels
/// 5. HighDynamic  — LRA proxy (max_block_lufs - min_block_lufs) > 20 LU
/// 6. Normal    — fallback
///
/// # Constraints
/// - libm only (no std::f32 methods)
/// - All thresholds are compile-time constants
/// - No heap allocation
pub fn detect_input_profile(pcm: &[f32], channels: u16) -> InputProfile {
    if pcm.is_empty() {
        return InputProfile::Silence;
    }

    let ch = channels as usize;

    // ── 1. Silence ──────────────────────────────────────────────────────────
    // RMS mean_square via Kahan — accurate for long files (§N1).
    let mean_sq = kahan_mean_square(pcm);
    if mean_sq < SILENCE_MEAN_SQUARE_THRESHOLD {
        return InputProfile::Silence;
    }

    // ── 2. DCOnly ────────────────────────────────────────────────────────────
    // Compute DC mean (sum / N). Low AC energy AND high DC mean → DCOnly.
    {
        let mut dc_sum: f32 = 0.0;
        for &s in pcm {
            dc_sum += s;
        }
        let dc_mean = dc_sum / (pcm.len() as f32);

        // Compute AC energy: mean_square of (sample - dc_mean)
        let mut ac_sum: f32 = 0.0;
        let mut ac_comp: f32 = 0.0;
        for &s in pcm {
            let centered = s - dc_mean;
            let y = centered * centered - ac_comp;
            let t = ac_sum + y;
            ac_comp = (t - ac_sum) - y;
            ac_sum = t;
        }
        let ac_mean_sq = ac_sum / (pcm.len() as f32);

        if libm::fabsf(dc_mean) > DC_BIAS_THRESHOLD
            && ac_mean_sq < DC_ONLY_MAX_AC_MEAN_SQUARE
        {
            return InputProfile::DCOnly;
        }
    }

    // ── 3. Clipped ───────────────────────────────────────────────────────────
    for &s in pcm {
        if libm::fabsf(s) >= CLIP_THRESHOLD {
            return InputProfile::Clipped;
        }
    }

    // ── 4. MonoInStereo ──────────────────────────────────────────────────────
    // Only meaningful for stereo input. Check every frame: L == R.
    if ch >= 2 {
        let mut is_mono_in_stereo = true;
        let frames = pcm.len() / ch;
        'outer: for f in 0..frames {
            let l = pcm[f * ch];
            let r = pcm[f * ch + 1];
            // Exact float equality: MonoInStereo means bit-identical channels.
            if l != r {
                is_mono_in_stereo = false;
                break 'outer;
            }
        }
        if is_mono_in_stereo {
            return InputProfile::MonoInStereo;
        }
    }

    // ── 5. HighDynamic (LRA proxy) ───────────────────────────────────────────
    // Approved Q1: max_block_lufs − min_block_lufs > 20 LU.
    // Block size: 400ms @ 48kHz (matches EBU R128 block size).
    // Only blocks above the absolute gate (-70 LUFS) are included.
    {
        let block_samples = if ch >= 2 {
            LUFS_BLOCK_SAMPLES_STEREO
        } else {
            LUFS_BLOCK_SAMPLES_MONO
        };

        // Collect per-block LUFS proxy values (no K-weighting — planning pass)
        let mut lufs_min: f32 = f32::MAX;
        let mut lufs_max: f32 = f32::MIN;
        let mut found_valid = false;

        let num_blocks = pcm.len() / block_samples;
        for b in 0..num_blocks {
            let start = b * block_samples;
            let end = start + block_samples;
            let block = &pcm[start..end];
            let block_ms = kahan_mean_square(block);
            if block_ms <= 0.0 {
                continue;
            }
            let block_lufs = -0.691 + 10.0 * libm::log10f(block_ms);
            // Apply absolute gate
            if block_lufs < LUFS_ABSOLUTE_GATE {
                continue;
            }
            if block_lufs < lufs_min {
                lufs_min = block_lufs;
            }
            if block_lufs > lufs_max {
                lufs_max = block_lufs;
            }
            found_valid = true;
        }

        if found_valid && (lufs_max - lufs_min) > HIGH_DYNAMIC_LU_THRESHOLD {
            return InputProfile::HighDynamic;
        }
    }

    // ── 6. Normal ────────────────────────────────────────────────────────────
    InputProfile::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a 440Hz sine wave at a given dBFS amplitude.
    fn sine_440hz(frames: usize, amplitude_linear: f32, channels: u16) -> alloc::vec::Vec<f32> {
        let ch = channels as usize;
        let sr = 48_000f32;
        let mut out = alloc::vec![0.0f32; frames * ch];
        for f in 0..frames {
            let s = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * f as f32 / sr)
                * amplitude_linear;
            for c in 0..ch {
                out[f * ch + c] = s;
            }
        }
        out
    }

    #[test]
    fn test_profile_silence() {
        // All-zero PCM → Silence
        let pcm = alloc::vec![0.0f32; 48_000 * 2]; // 1s stereo
        assert_eq!(detect_input_profile(&pcm, 2), InputProfile::Silence);
    }

    #[test]
    fn test_profile_empty() {
        // Empty slice → Silence (early-exit guard)
        assert_eq!(detect_input_profile(&[], 2), InputProfile::Silence);
    }

    #[test]
    fn test_profile_normal() {
        // 2s of 440Hz at -6 dBFS stereo with DISTINCT L/R channels → Normal.
        // L = 440Hz, R = 880Hz — different frequencies so L ≠ R on almost
        // every frame, preventing MonoInStereo classification.
        let amplitude = libm::powf(10.0, -6.0 / 20.0);
        let sr = 48_000f32;
        let frames = 48_000usize * 2;
        let mut pcm = alloc::vec![0.0f32; frames * 2];
        for f in 0..frames {
            let l = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * f as f32 / sr) * amplitude;
            let r = libm::sinf(2.0 * core::f32::consts::PI * 880.0 * f as f32 / sr) * amplitude;
            pcm[f * 2]     = l;
            pcm[f * 2 + 1] = r;
        }
        assert_eq!(detect_input_profile(&pcm, 2), InputProfile::Normal);
    }

    #[test]
    fn test_profile_clipped() {
        // One sample exactly at 1.0 in an otherwise normal signal → Clipped
        let mut pcm = sine_440hz(48_000, 0.5, 2);
        // Insert a clipped sample at frame 100, channel 0
        pcm[100 * 2] = 1.0;
        assert_eq!(detect_input_profile(&pcm, 2), InputProfile::Clipped);
    }

    #[test]
    fn test_profile_dc_only() {
        // Constant DC signal at 0.5 → DCOnly
        // DC mean = 0.5 > DC_BIAS_THRESHOLD (0.01)
        // AC energy ≈ 0.0 < DC_ONLY_MAX_AC_MEAN_SQUARE (1e-8)
        let pcm = alloc::vec![0.5f32; 48_000 * 2];
        assert_eq!(detect_input_profile(&pcm, 2), InputProfile::DCOnly);
    }

    #[test]
    fn test_profile_mono_in_stereo() {
        // Stereo PCM where L == R every frame → MonoInStereo
        let amplitude = libm::powf(10.0, -6.0 / 20.0);
        // Build interleaved stereo with identical channels
        let sr = 48_000f32;
        let frames = 48_000usize;
        let mut pcm = alloc::vec![0.0f32; frames * 2];
        for f in 0..frames {
            let s = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * f as f32 / sr)
                * amplitude;
            pcm[f * 2] = s;     // L
            pcm[f * 2 + 1] = s; // R (identical)
        }
        assert_eq!(detect_input_profile(&pcm, 2), InputProfile::MonoInStereo);
    }
}
