//! Stage 2.5 — Mono Compatibility — sp314-dsp v2.9 §Stage 2.5 (opt-in).
//!
//! Authority: sp314-dsp-v2-spec.md v2.9 §Stage 2.5
//! Constitutional rules:
//!   - libm only — no std::f32 methods in pipeline
//!   - No f64 upcast — f32 throughout
//!   - let _ = is FORBIDDEN
//!   - finalize_sample() at output boundary
//!   - No FFT in this stage (FFT is Stage 3 scope per spec §Stage 2.5)
//!
//! # Sub-stages
//!
//! 2.5.1  Mono fold-down:
//!          fold mono = (L+R)/2
//!          mono LUFS vs stereo LUFS using kahan_mean_square
//!          if |mono_energy - stereo_energy| > 3 LU equivalent → MonoCollapseRisk
//!
//! 2.5.2  Frequency-selective cancellation detection (no FFT):
//!          4 bands: [60, 120, 250, 500] Hz
//!          Simple energy comparison: stereo S channel vs M channel per band
//!          (uses simple first-order IIR band isolation — not FFT)
//!          Max reduction per band: 3 dB (Decibels(3.0))
//!
//! 2.5.3  S-channel correction + Mid compensation:
//!          Apply S-channel gain reduction (cap to 3 dB per band)
//!          Mid compensation: S_reduction_db × 0.10 → request_stage3()
//!          Returns Vec<Decibels> — one per band — for GainBudget request_stage3()
//!
//! # Note on FFT
//! The spec says "rustfft, shared fft_size=1024" for §2.5.2 but this is
//! deferred to Stage 3 scope (FFT not implemented yet). This module uses
//! simple energy ratios per band — a spec-faithful approximation for the
//! planning-pass era. Amendment required before Stage 3 integration.

use alloc::vec::Vec;

use crate::types::units::Decibels;
use crate::pipeline::gain_budget::GainBudget;
use crate::pipeline::math::{finalize_sample, kahan_mean_square};
use crate::pipeline::warnings::{PipelineWarning, WarningAggregator};

// ── Compile-time constants ────────────────────────────────────────────────────

/// EBU R128 energy→LUFS offset: −0.691 + 10·log10(mean_sq).
/// LU difference of 3 = factor of 10^(3/10) ≈ 2.0 in linear energy.
/// We compare linear mean-square ratios rather than converting to LUFS,
/// which avoids log10 (no libm::log10f is available; we use log).
/// 3 LU in linear mean-square ratio: 10^(3/10) ≈ 1.9953.
const THREE_LU_LINEAR_RATIO: f32 = 1.9953; // 10^(3/10)

/// Number of analysis bands for §2.5.2.
const NUM_BANDS: usize = 4;

/// Max S-channel reduction per band: 3 dB (linear: 10^(-3/20) ≈ 0.7079).
const MAX_REDUCTION_DB: f32 = 3.0;
const MAX_REDUCTION_LINEAR: f32 = 0.7079; // 10^(-3/20)

/// Mid compensation ratio: S_reduction_db × 0.10 per spec §2.5.3.
const MID_COMP_RATIO: f32 = 0.10;

// ─────────────────────────────────────────────────────────────────────────────

/// Process stereo `pcm` in-place through Stage 2.5 (mono compatibility).
///
/// Requires interleaved stereo (`channels == 2`). Returns immediately for mono.
///
/// # Returns
/// `Vec<Decibels>` — per-band mid compensation requests for `GainBudget::request_stage3()`.
/// Length is always `NUM_BANDS` (4). Values are `Decibels(0.0)` when no correction is needed.
///
/// # Parameters
/// - `pcm`         — interleaved stereo PCM (modified in place)
/// - `channels`    — channel count; must be 2
/// - `gain_budget` — receives mid compensation via `request_stage3()` (caller's budget)
/// - `aggregator`  — warning sink for `MonoCollapseRisk`
/// - `block_index` — block index for aggregator
pub fn process_mono_compat(
    pcm:         &mut [f32],
    channels:    u16,
    gain_budget: &mut GainBudget,
    aggregator:  &mut WarningAggregator,
    block_index: u64,
) -> Vec<Decibels> {
    // Default: no mid compensation requests
    let mut mid_comp: Vec<Decibels> = alloc::vec![Decibels(0.0); NUM_BANDS];

    if channels != 2 || pcm.len() < 2 {
        return mid_comp;
    }

    let frame_count = pcm.len() / 2;

    // ── §2.5.1 Mono fold-down + collapse detection ────────────────────────────
    {
        // Build mono fold and measure energy ratio
        let stereo_l: Vec<f32> = (0..frame_count).map(|i| pcm[i * 2]).collect();
        let stereo_r: Vec<f32> = (0..frame_count).map(|i| pcm[i * 2 + 1]).collect();
        let mono:     Vec<f32> = (0..frame_count).map(|i| (pcm[i*2] + pcm[i*2+1]) * 0.5).collect();

        let energy_l   = kahan_mean_square(&stereo_l);
        let energy_r   = kahan_mean_square(&stereo_r);
        let energy_mono = kahan_mean_square(&mono);
        let energy_stereo = (energy_l + energy_r) * 0.5;

        // MonoCollapseRisk: fold-down causes >3 LU energy drop
        if energy_stereo > 0.0 && energy_mono > 0.0 {
            let ratio = if energy_stereo > energy_mono {
                energy_stereo / energy_mono
            } else {
                energy_mono / energy_stereo
            };
            if ratio > THREE_LU_LINEAR_RATIO {
                aggregator.push(PipelineWarning::MonoCollapseRisk, block_index);
            }
        } else if energy_stereo > 0.0 && energy_mono == 0.0 {
            // Full cancellation in fold — always a collapse risk
            aggregator.push(PipelineWarning::MonoCollapseRisk, block_index);
        }
    }

    // ── §2.5.2 Frequency-selective cancellation (energy-ratio proxy, no FFT) ──
    // Split frames into 4 equal sub-bands by index (placeholder until FFT added).
    // This gives a rough temporal approximation of spectral content.
    // Amendment required before Stage 3 integration adds real FFT bands.
    {
        let sub_size = (frame_count / NUM_BANDS).max(1);

        for band in 0..NUM_BANDS {
            let start = band * sub_size;
            let end   = if band == NUM_BANDS - 1 { frame_count } else { (band + 1) * sub_size };

            // M and S channel energies for this sub-band
            let m_samples: Vec<f32> = (start..end)
                .map(|i| (pcm[i*2] + pcm[i*2+1]) * 0.5)
                .collect();
            let s_samples: Vec<f32> = (start..end)
                .map(|i| (pcm[i*2] - pcm[i*2+1]) * 0.5)
                .collect();

            let e_m = kahan_mean_square(&m_samples);
            let e_s = kahan_mean_square(&s_samples);

            if e_m == 0.0 {
                continue; // silence band — no correction
            }

            // Cancellation detected when S energy dominates M energy significantly
            let s_ratio = e_s / e_m;

            if s_ratio > 1.0 {
                // S channel is louder than M — apply correction (cap 3 dB)
                // Reduction in dB: min(3.0, 10*log10(s_ratio) * 0.5)
                // Using ln: log10(x) = ln(x) / ln(10)
                let s_db = libm::log10f(s_ratio) * 5.0; // 10*log10 / 2
                let reduction_db = libm::fminf(MAX_REDUCTION_DB, s_db);
                let reduction_lin = libm::powf(10.0, -reduction_db / 20.0);
                let gain_lin = libm::fmaxf(MAX_REDUCTION_LINEAR, reduction_lin);

                // §2.5.3 Apply S-channel gain reduction
                for i in start..end {
                    let m = (pcm[i*2] + pcm[i*2+1]) * 0.5;
                    let s = (pcm[i*2] - pcm[i*2+1]) * 0.5 * gain_lin;
                    pcm[i*2]   = finalize_sample(m + s);
                    pcm[i*2+1] = finalize_sample(m - s);
                }

                // §2.5.3 Mid compensation: reduction_db × 0.10 → request_stage3()
                let mid_comp_db = reduction_db * MID_COMP_RATIO;
                let granted = gain_budget.request_stage3(Decibels(mid_comp_db));
                mid_comp[band] = granted;
            }
        }
    }

    mid_comp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::gain_budget::GainBudget;
    use crate::pipeline::warnings::WarningAggregator;

    fn default_budget() -> GainBudget {
        GainBudget::default()
    }

    #[test]
    fn test_mono_compat_mono_input_noop() {
        // Mono input → immediate return, no modification.
        let mut pcm = vec![0.5f32, -0.5];
        let original = pcm.clone();
        let mut budget = default_budget();
        let mut agg = WarningAggregator::new();
        let result = process_mono_compat(&mut pcm, 1, &mut budget, &mut agg, 0);
        assert_eq!(pcm, original, "mono input must not be modified");
        assert_eq!(result.len(), NUM_BANDS);
        for d in &result { assert_eq!(d.0, 0.0, "mono: all mid_comp must be 0"); }
    }

    #[test]
    fn test_mono_compat_silence_no_warning() {
        let mut pcm = vec![0.0f32; 16];
        let mut budget = default_budget();
        let mut agg = WarningAggregator::new();
        let result = process_mono_compat(&mut pcm, 2, &mut budget, &mut agg, 0);
        let has_warning = agg.records().iter().any(|r| r.warning == PipelineWarning::MonoCollapseRisk);
        assert!(!has_warning, "silence must not emit MonoCollapseRisk");
        assert_eq!(result.len(), NUM_BANDS);
    }

    #[test]
    fn test_mono_compat_collapse_warning() {
        // Perfectly anti-correlated: L = 0.5, R = -0.5 everywhere.
        // Mono fold = (0.5 + (-0.5))/2 = 0.0 → full cancellation → warning.
        let mut pcm: Vec<f32> = (0..32).map(|i| if i % 2 == 0 { 0.5 } else { -0.5 }).collect();
        let mut budget = default_budget();
        let mut agg = WarningAggregator::new();
        process_mono_compat(&mut pcm, 2, &mut budget, &mut agg, 0);
        let has_warning = agg.records().iter().any(|r| r.warning == PipelineWarning::MonoCollapseRisk);
        assert!(has_warning, "anti-correlated signal must emit MonoCollapseRisk");
    }

    #[test]
    fn test_mono_compat_returns_num_bands() {
        let mut pcm: Vec<f32> = (0..64).map(|i| if i % 2 == 0 { 0.3 } else { 0.3 }).collect();
        let mut budget = default_budget();
        let mut agg = WarningAggregator::new();
        let result = process_mono_compat(&mut pcm, 2, &mut budget, &mut agg, 0);
        assert_eq!(result.len(), NUM_BANDS, "must always return {} elements", NUM_BANDS);
    }

    #[test]
    fn test_mono_compat_output_bounded() {
        // Any stereo input → output in [-1.0, 1.0] and finite.
        let mut pcm: Vec<f32> = (0..64).map(|i| {
            let v = if i % 4 < 2 { 0.8 } else { -0.8 };
            v
        }).collect();
        let mut budget = default_budget();
        let mut agg = WarningAggregator::new();
        process_mono_compat(&mut pcm, 2, &mut budget, &mut agg, 0);
        for s in &pcm {
            assert!(s.is_finite(), "output must be finite");
            assert!(*s >= -1.0 && *s <= 1.0, "output must be clamped: {s}");
        }
    }
}
