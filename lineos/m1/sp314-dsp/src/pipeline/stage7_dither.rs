//! Stage 7 — Dithering & Export Prep — sp314-dsp v2.9 §Stage 7.
//!
//! Call order (FIXED): 7.1 → 7.2 → 7.3 → 7.4
//!
//! **7.1** Noise shaping (F-weighted) — only when `dither_bits < 32`.
//!   Simplified: single-pole IIR noise shaper.
//!   Full F-weighted noise shaping kernel deferred to Sandbox step \[14+\].
//!
//! **7.2** TPDF dither — deterministic LCG seeded from `preset.dither_seed`.
//!   Skipped when `dither_bits == 32` (raw/float export).
//!   16-bit: ±1 LSB = 1.0 / 32768.0
//!   24-bit: ±1 LSB = 1.0 / 8388608.0
//!   LCG: Knuth MMIX multiplier + Fibonacci addend.
//!   Same seed → identical dither sequence every call. Always.
//!
//! **7.3** Final true peak verification (2nd pass, simplified).
//!   Full 4× oversampled FIR pass deferred to Sandbox step \[14+\].
//!   Proxy: max |sample| check.
//!
//! **7.4** Clip detection: any |sample| >= 1.0 → `clip_detected = true` + warning.
//!
//! # Determinism guarantee
//! LCG state is reset to `preset.dither_seed` at the START of every call.
//! No `rand::thread_rng()`. No entropy. No OS calls. Bit-identical output
//! for identical (pcm, seed) pairs across all platforms.

use crate::pipeline::math::finalize_sample;
use crate::pipeline::warnings::{PipelineWarning, WarningAggregator};
use crate::types::mastering_preset::MasteringPreset;

// ── Constants ─────────────────────────────────────────────────────────────────

/// 16-bit ±1 LSB amplitude.
const AMP_16BIT: f32 = 1.0 / 32_768.0;
/// 24-bit ±1 LSB amplitude.
const AMP_24BIT: f32 = 1.0 / 8_388_608.0;

/// LCG multiplier: Knuth MMIX.
const LCG_MUL: u64 = 6_364_136_223_846_793_005;
/// LCG addend: Fibonacci-derived.
const LCG_ADD: u64 = 1_442_695_040_888_963_407;

/// Noise shaper feedback coefficient (single-pole IIR).
/// Chosen for gentle first-order noise shaping: 0.5 pushes error energy toward Nyquist.
const NS_COEFF: f32 = 0.5;

// ── LCG ──────────────────────────────────────────────────────────────────────

/// Advance LCG state one step and return next state.
#[inline(always)]
fn lcg_next(state: u64) -> u64 {
    state.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD)
}

/// Extract a normalized f32 dither value in [-1.0, 1.0] from LCG state.
/// Uses bits 33..64 (upper 31 bits) for uniformity.
#[inline(always)]
fn lcg_f32(state: u64) -> f32 {
    (state >> 33) as f32 / i32::MAX as f32
}

// ── process_dither ────────────────────────────────────────────────────────────

/// Process `pcm` in-place through Stage 7 (7.1→7.2→7.3→7.4).
///
/// # Returns
/// `clip_detected: bool` — true if any sample |x| >= 1.0 after processing.
pub fn process_dither(
    pcm:        &mut [f32],
    preset:     &MasteringPreset,
    aggregator: &mut WarningAggregator,
) -> bool {
    if pcm.is_empty() { return false; }

    let skip_dither = preset.dither_bits == 32;

    // Noise shaper state — reset per call for determinism
    let mut ns_error: f32 = 0.0;

    // Reset LCG to seed — MUST be first, before any LCG call
    let mut lcg_state: u64 = preset.dither_seed;

    // TPDF amplitude
    let amplitude = if preset.dither_bits == 16 { AMP_16BIT } else { AMP_24BIT };

    for sample in pcm.iter_mut() {
        let x = *sample;

        // ── §7.1 Noise shaping (single-pole IIR, skipped at 32-bit) ──────────
        let shaped = if skip_dither {
            x
        } else {
            // Apply noise shaper: subtract fed-back quantization error
            x - ns_error * NS_COEFF
        };

        // ── §7.2 TPDF dither (skipped at 32-bit) ─────────────────────────────
        let dithered = if skip_dither {
            shaped
        } else {
            // Two independent LCG steps → TPDF (triangular distribution)
            lcg_state       = lcg_next(lcg_state);
            let r1          = lcg_f32(lcg_state);
            lcg_state       = lcg_next(lcg_state);
            let r2          = lcg_f32(lcg_state);
            let tpdf        = (r1 - r2) * amplitude;
            let out         = shaped + tpdf;
            // Update noise shaper error for next sample
            ns_error        = out - shaped;
            out
        };

        *sample = finalize_sample(dithered);
    }

    // ── §7.3 Final true peak verification (proxy — FIR deferred) ─────────────
    // Full 4× oversampled FIR true peak pass deferred to Sandbox step [14+].
    // Proxy: scan for any |sample| >= 1.0.

    // ── §7.4 Clip detection ───────────────────────────────────────────────────
    let mut clip_detected = false;
    for &s in pcm.iter() {
        if libm::fabsf(s) >= 1.0 {
            clip_detected = true;
            break;
        }
    }
    if clip_detected {
        // Emit warning — block_index 0 (Stage 7 is post-block, full-mix pass)
        aggregator.push(PipelineWarning::LufsTargetMiss, 0); // repurposed: clip event
        // Note: a ClipDetected variant is not in PipelineWarning v2.9.
        // This is a known gap — amendment required to add ClipDetected variant.
        // For now, clip_detected flag is the primary signal; warning is informational.
    }

    clip_detected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::warnings::WarningAggregator;
    use crate::types::mastering_preset::{
        CompressionStyle, EqCurve, LimiterAlgorithm, MasteringPreset, SaturationStyle,
    };

    fn preset_24bit(seed: u64) -> MasteringPreset {
        MasteringPreset {
            lufs_target:    -14.0,
            true_peak_ceil: -1.0,
            compression:    CompressionStyle::Transparent,
            saturation:     SaturationStyle::None,
            eq_curve:       EqCurve::Flat,
            limiter:        LimiterAlgorithm::Transparent,
            gain_budget_db: 6.0,
            dither_bits:    24,
            dither_seed:    seed,
            oversampling:   4,
        }
    }

    fn preset_16bit(seed: u64) -> MasteringPreset {
        MasteringPreset { dither_bits: 16, ..preset_24bit(seed) }
    }

    fn preset_32bit() -> MasteringPreset {
        MasteringPreset { dither_bits: 32, ..preset_24bit(0xDEAD_u64) }
    }

    #[test]
    fn test_dither_deterministic_same_seed() {
        // Two calls with identical (pcm, seed) must produce bit-identical output.
        let make = || -> alloc::vec::Vec<f32> {
            (0..256).map(|i| 0.5 * libm::sinf(
                2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0
            )).collect()
        };
        let mut pcm1 = make();
        let mut pcm2 = make();
        let mut agg1 = WarningAggregator::new();
        let mut agg2 = WarningAggregator::new();
        process_dither(&mut pcm1, &preset_24bit(0xABCD), &mut agg1);
        process_dither(&mut pcm2, &preset_24bit(0xABCD), &mut agg2);
        assert_eq!(pcm1, pcm2, "same seed → identical dither sequence");
    }

    #[test]
    fn test_dither_different_seeds_differ() {
        // Different seeds must produce different dither sequences.
        // Use 16-bit dither (AMP_16BIT ≈ 3e-5, above finalize_sample quantization floor 1e-6)
        // on a silence input so dither value alone determines output.
        let mut pcm1 = alloc::vec![0.0f32; 256];
        let mut pcm2 = alloc::vec![0.0f32; 256];
        let mut agg = WarningAggregator::new();
        process_dither(&mut pcm1, &preset_16bit(0x1111_u64), &mut agg);
        process_dither(&mut pcm2, &preset_16bit(0x2222_u64), &mut agg);
        assert_ne!(pcm1, pcm2, "different seeds must produce different dither");
    }

    #[test]
    fn test_dither_32bit_exact_passthrough() {
        // 32-bit preset → dither skipped → output == finalize_sample(input)
        let mut pcm: alloc::vec::Vec<f32> = (0..256)
            .map(|i| 0.3 * libm::sinf(
                2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0
            )).collect();
        let expected: alloc::vec::Vec<f32> = pcm.iter().map(|&s| finalize_sample(s)).collect();
        let mut agg = WarningAggregator::new();
        process_dither(&mut pcm, &preset_32bit(), &mut agg);
        assert_eq!(pcm, expected, "32-bit preset must pass through unchanged (modulo finalize)");
    }

    #[test]
    fn test_dither_amplitude_16bit_bounded() {
        // 16-bit dither must not add more than ±1 LSB × 2 to silence.
        let mut pcm = alloc::vec![0.0f32; 1024];
        let mut agg = WarningAggregator::new();
        process_dither(&mut pcm, &preset_16bit(42), &mut agg);
        let max_amp = pcm.iter().map(|s| libm::fabsf(*s)).fold(0.0f32, f32::max);
        assert!(max_amp <= AMP_16BIT * 2.0 + 1e-9,
            "16-bit dither on silence must stay within ±2 LSB: max={max_amp}");
    }

    #[test]
    fn test_dither_amplitude_24bit_bounded() {
        // 24-bit dither must not add more than ±1 LSB × 2 to silence.
        let mut pcm = alloc::vec![0.0f32; 1024];
        let mut agg = WarningAggregator::new();
        process_dither(&mut pcm, &preset_24bit(42), &mut agg);
        let max_amp = pcm.iter().map(|s| libm::fabsf(*s)).fold(0.0f32, f32::max);
        assert!(max_amp <= AMP_24BIT * 2.0 + 1e-9,
            "24-bit dither on silence must stay within ±2 LSB: max={max_amp}");
    }

    #[test]
    fn test_dither_output_bounded() {
        // Dithered output for a non-clipping signal must remain in [-1, 1].
        let mut pcm: alloc::vec::Vec<f32> = (0..256)
            .map(|i| 0.9 * libm::sinf(
                2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0
            )).collect();
        let mut agg = WarningAggregator::new();
        let clip = process_dither(&mut pcm, &preset_24bit(0xDEAD), &mut agg);
        assert!(!clip, "well-below-unity signal must not clip");
        for s in &pcm {
            assert!(s.is_finite() && *s >= -1.0 && *s <= 1.0,
                "output must be in [-1, 1]: {s}");
        }
    }

    #[test]
    fn test_dither_silence_no_clip() {
        let mut pcm = alloc::vec![0.0f32; 512];
        let mut agg = WarningAggregator::new();
        let clip = process_dither(&mut pcm, &preset_24bit(0), &mut agg);
        assert!(!clip, "silence must not clip");
    }

    #[test]
    fn test_dither_clip_detection_unity() {
        // A unity-amplitude signal must trigger clip_detected.
        let mut pcm = alloc::vec![1.0f32; 64];
        let mut agg = WarningAggregator::new();
        let clip = process_dither(&mut pcm, &preset_32bit(), &mut agg);
        assert!(clip, "unity-amplitude signal must trigger clip_detected");
    }

    #[test]
    fn test_dither_lcg_constants() {
        // Verify LCG produces non-zero output for non-zero seed.
        let s1 = lcg_next(0xDEAD_BEEF_u64);
        let s2 = lcg_next(s1);
        assert_ne!(s1, 0xDEAD_BEEF_u64, "LCG must advance state");
        assert_ne!(s2, s1, "consecutive LCG states must differ");
        assert!(lcg_f32(s1).is_finite(), "LCG f32 must be finite");
    }
}
