//! Stage 3 — EQ — sp314-dsp v2.9 §Stage 3.
//!
//! Authority: sp314-dsp-v2-spec.md v2.9 §Stage 3
//! Constitutional rules:
//!   - libm only — no std::f32 methods in pipeline
//!   - No f64 upcast — f32 throughout
//!   - let _ = is FORBIDDEN
//!   - finalize_sample() at output boundary
//!   - Call order 3.1→3.2→3.3→3.4 is FIXED — NEVER change
//!
//! # Sub-stages (FIXED call order)
//!
//! **3.1** High-pass filter: 4th-order Butterworth at 20Hz.
//! Always active, no GainBudget interaction.
//! Implemented as 2× cascaded 2nd-order biquad HPF.
//! Q = 0.5412 (stage 1) and Q = 1.3066 (stage 2) for maximally-flat Butterworth.
//!
//! **3.2** Linear phase EQ (biquad shelves — full FFT deferred):
//! Air = high-shelf +1.5 dB at 12kHz, Warmth = low-shelf +1.0 dB at 180Hz,
//! Presence = peaking +1.0 dB at 3kHz, Flat = passthrough (no request).
//! Full rustfft linear phase EQ (fft_size=1024) is deferred to Sandbox
//! step 14+. Amendment required before FFT wiring.
//!
//! **3.3** Dynamic EQ — 4 bands at 80, 250, 2000, 8000 Hz; 2:1 ratio; 5ms/80ms.
//! Applied when band energy exceeds -18 dBFS threshold. No GainBudget.
//!
//! **3.4** Air band enhancement (Tape/Tube saturation styles only).
//! Calls request_stage3(Decibels(1.0)). If capped: pushes InsightHint.
//! InsightHint = "Air band enhancement partially suppressed by budget cap".

use alloc::vec::Vec;

use crate::types::units::Decibels;
use crate::types::mastering_preset::{EqCurve, MasteringPreset, SaturationStyle};
use crate::pipeline::gain_budget::GainBudget;
use crate::pipeline::math::finalize_sample;
use crate::pipeline::warnings::WarningAggregator;
use crate::dsp::biquad::Biquad;

// ── Compile-time constants ────────────────────────────────────────────────────

/// HPF cutoff frequency per spec §Stage 3.1.
const HPF_FREQ_HZ: f32 = 20.0;

/// Butterworth 4th-order Q factors for 2-stage cascade.
/// Stage 1 Q = 1/(2·cos(67.5°)) = 0.5412, Stage 2 Q = 1/(2·cos(22.5°)) = 1.3066.
const HPF_Q1: f32 = 0.5412;
const HPF_Q2: f32 = 1.3066;

/// §3.2 Air shelf: +1.5 dB @ 12 kHz.
const AIR_GAIN_DB:  f32 = 1.5;
const AIR_FREQ_HZ:  f32 = 12_000.0;
const AIR_Q:        f32 = 0.707;

/// §3.2 Warmth shelf: +1.0 dB @ 180 Hz.
const WARMTH_GAIN_DB: f32 = 1.0;
const WARMTH_FREQ_HZ: f32 = 180.0;
const WARMTH_Q:       f32 = 0.707;

/// §3.2 Presence bell: +1.0 dB @ 3 kHz, 1-octave bandwidth.
const PRESENCE_GAIN_DB: f32 = 1.0;
const PRESENCE_FREQ_HZ: f32 = 3_000.0;
const PRESENCE_BW:      f32 = 1.0; // octaves

/// §3.3 Dynamic EQ band center frequencies (Hz).
const DYN_BAND_FREQS: [f32; 4] = [80.0, 250.0, 2_000.0, 8_000.0];

/// §3.3 Threshold for dynamic EQ activation: -18 dBFS energy.
/// Linear amplitude threshold: 10^(-18/20) ≈ 0.1259.
const DYN_THRESHOLD_LINEAR: f32 = 0.1259;

/// §3.3 Attack coefficient (5ms @ 48kHz): 1 - exp(-1 / (0.005 * 48000)).
const DYN_ATTACK_MS:  f32 = 5.0;
/// §3.3 Release coefficient (80ms @ 48kHz): 1 - exp(-1 / (0.08 * 48000)).
const DYN_RELEASE_MS: f32 = 80.0;

/// §3.3 Compression ratio 2:1 — gain reduction slope = (1 - 1/ratio) = 0.5.
const DYN_RATIO_SLOPE: f32 = 0.5; // = 1 - 1/2

/// §3.4 Air band enhancement gain request.
const AIR_ENHANCE_GAIN_DB: f32 = 1.0;

/// §3.4 InsightHint — static string (§Stage 8.10: "Static strings only").
pub const INSIGHT_AIR_BAND_CAPPED: &str =
    "Air band enhancement partially suppressed by budget cap";

// ── Process function ──────────────────────────────────────────────────────────

/// Process `pcm` in-place through Stage 3 EQ (call order 3.1→3.2→3.3→3.4 fixed).
///
/// # Parameters
/// - `pcm`                  — interleaved PCM samples (modified in place)
/// - `channels`             — channel count (1 or 2)
/// - `sample_rate`          — sample rate in Hz (expected: 48000)
/// - `preset`               — provides `EqCurve` (3.2) and `SaturationStyle` (3.4)
/// - `gain_budget`          — shared gain budget; receives `request_stage3()` calls
/// - `aggregator`           — warning sink (currently unused by Stage 3, reserved)
/// - `block_index`          — current block index (for aggregator)
/// - `stage3_compensation`  — per-band mid compensation from Stage 2.5; applied to budget first
/// - `insight_hints`        — caller-owned `Vec<&'static str>` for Stage 3.4 InsightHints
pub fn process_eq(
    pcm:                 &mut [f32],
    channels:            u16,
    sample_rate:         f32,
    preset:              &MasteringPreset,
    gain_budget:         &mut GainBudget,
    _aggregator:         &mut WarningAggregator,
    _block_index:        u64,
    stage3_compensation: &[Decibels],
    insight_hints:       &mut Vec<&'static str>,
) {
    if pcm.is_empty() {
        return;
    }

    let ch = channels as usize;
    let frame_count = if ch > 0 { pcm.len() / ch } else { return };

    // ── Apply Stage 2.5 mid compensation to budget first ─────────────────────
    // These are pre-granted in Stage 2.5; re-record them against Stage 3 budget
    // so the total allocation is consistent. If budget is already consumed, skip.
    for comp in stage3_compensation {
        if comp.0 > 0.0 {
            gain_budget.request_stage3(*comp);
        }
    }

    // ── §3.1 High-pass filter (4th-order Butterworth @ 20Hz) ─────────────────
    {
        // Two cascaded 2nd-order HPF biquads per channel.
        let mut hpf1: Vec<Biquad> = (0..ch).map(|_| {
            let mut b = Biquad::new();
            b.set_hpf(HPF_FREQ_HZ, sample_rate, HPF_Q1);
            b
        }).collect();
        let mut hpf2: Vec<Biquad> = (0..ch).map(|_| {
            let mut b = Biquad::new();
            b.set_hpf(HPF_FREQ_HZ, sample_rate, HPF_Q2);
            b
        }).collect();

        for frame_idx in 0..frame_count {
            for c in 0..ch {
                let i = frame_idx * ch + c;
                let y1 = hpf1[c].process(pcm[i]);
                let y2 = hpf2[c].process(y1);
                pcm[i] = finalize_sample(y2);
            }
        }
    }

    // ── §3.2 Linear phase EQ (biquad — FFT deferred) ─────────────────────────
    {
        let eq_gain_db = match preset.eq_curve {
            EqCurve::Air      => Some((AIR_GAIN_DB, Decibels(AIR_GAIN_DB))),
            EqCurve::Warmth   => Some((WARMTH_GAIN_DB, Decibels(WARMTH_GAIN_DB))),
            EqCurve::Presence => Some((PRESENCE_GAIN_DB, Decibels(PRESENCE_GAIN_DB))),
            EqCurve::Flat | EqCurve::Broadcast => None,
        };

        if let Some((_, budget_req)) = eq_gain_db {
            gain_budget.request_stage3(budget_req);

            let mut eq_filters: Vec<Biquad> = (0..ch).map(|_| {
                let mut b = Biquad::new();
                match preset.eq_curve {
                    EqCurve::Air =>
                        b.set_high_shelf(AIR_FREQ_HZ, sample_rate, AIR_GAIN_DB, AIR_Q),
                    EqCurve::Warmth =>
                        b.set_high_shelf(WARMTH_FREQ_HZ, sample_rate, -WARMTH_GAIN_DB, WARMTH_Q),
                    EqCurve::Presence =>
                        b.set_peaking(PRESENCE_FREQ_HZ, sample_rate, PRESENCE_GAIN_DB, PRESENCE_BW),
                    _ => {}
                }
                b
            }).collect();

            for frame_idx in 0..frame_count {
                for c in 0..ch {
                    let i = frame_idx * ch + c;
                    pcm[i] = finalize_sample(eq_filters[c].process(pcm[i]));
                }
            }
        }
    }

    // ── §3.3 Dynamic EQ (4 bands, 2:1, 5ms/80ms) — no budget ────────────────
    {
        // Pre-compute attack/release coefficients from ms
        let attack_coeff  = 1.0 - libm::expf(-1.0 / (DYN_ATTACK_MS  * 0.001 * sample_rate));
        let release_coeff = 1.0 - libm::expf(-1.0 / (DYN_RELEASE_MS * 0.001 * sample_rate));

        // Per-channel per-band envelope followers and LPF isolators
        let mut env:  Vec<[f32; 4]> = vec![[0.0f32; 4]; ch];
        // Simple one-pole IIR band isolator state per channel per band
        let mut lpf_state: Vec<[f32; 4]> = vec![[0.0f32; 4]; ch];

        // Band IIR coefficients: one-pole LPF at each crossover
        // coeff = exp(-2π * fc / fs)
        let band_coeffs: [f32; 4] = DYN_BAND_FREQS.map(|f| {
            libm::expf(-2.0 * core::f32::consts::PI * f / sample_rate)
        });

        for frame_idx in 0..frame_count {
            for c in 0..ch {
                let i = frame_idx * ch + c;
                let x = pcm[i];
                let mut gain_acc: f32 = 1.0;

                for band in 0..4 {
                    // One-pole IIR band isolation (low-pass at band frequency)
                    let a = band_coeffs[band];
                    lpf_state[c][band] = a * lpf_state[c][band] + (1.0 - a) * x;
                    let band_sig = lpf_state[c][band];
                    let band_amp = libm::fabsf(band_sig);

                    // Envelope follower
                    let coeff = if band_amp > env[c][band] { attack_coeff } else { release_coeff };
                    env[c][band] = env[c][band] + coeff * (band_amp - env[c][band]);

                    // Apply 2:1 compression above threshold
                    if env[c][band] > DYN_THRESHOLD_LINEAR {
                        let excess = env[c][band] - DYN_THRESHOLD_LINEAR;
                        // Gain reduction: reduce by (excess * DYN_RATIO_SLOPE)
                        let gr = 1.0 / (1.0 + excess * DYN_RATIO_SLOPE / DYN_THRESHOLD_LINEAR);
                        gain_acc *= gr;
                    }
                }

                pcm[i] = finalize_sample(x * gain_acc);
            }
        }
    }

    // ── §3.4 Air band enhancement (Tape/Tube only) ───────────────────────────
    {
        let is_tape_or_tube = matches!(
            preset.saturation,
            SaturationStyle::Tape | SaturationStyle::Tube | SaturationStyle::Both
        );

        if is_tape_or_tube {
            let requested = Decibels(AIR_ENHANCE_GAIN_DB);
            let granted   = gain_budget.request_stage3(requested);

            if granted.0 < requested.0 {
                // Budget cap — emit InsightHint (static string only per §Stage 8.10)
                if !insight_hints.contains(&INSIGHT_AIR_BAND_CAPPED) {
                    insight_hints.push(INSIGHT_AIR_BAND_CAPPED);
                }
            }

            if granted.0 > 0.0 {
                // Apply air band boost: high-shelf gain on granted amount
                let mut air: Vec<Biquad> = (0..ch).map(|_| {
                    let mut b = Biquad::new();
                    b.set_high_shelf(AIR_FREQ_HZ, sample_rate, granted.0, AIR_Q);
                    b
                }).collect();

                for frame_idx in 0..frame_count {
                    for c in 0..ch {
                        let i = frame_idx * ch + c;
                        pcm[i] = finalize_sample(air[c].process(pcm[i]));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::gain_budget::GainBudget;
    use crate::pipeline::warnings::WarningAggregator;
    use crate::types::mastering_preset::{
        CompressionStyle, LimiterAlgorithm, MasteringPreset, SaturationStyle,
    };

    fn preset(eq: EqCurve, sat: SaturationStyle) -> MasteringPreset {
        MasteringPreset {
            lufs_target:    -14.0,
            true_peak_ceil: -1.0,
            compression:    CompressionStyle::Transparent,
            saturation:     sat,
            eq_curve:       eq,
            limiter:        LimiterAlgorithm::Transparent,
            gain_budget_db: 6.0,
            dither_bits:    24,
            dither_seed:    0xDEAD,
            oversampling:   4,
        }
    }

    fn run(pcm: &mut [f32], eq: EqCurve, sat: SaturationStyle) -> Vec<&'static str> {
        let mut budget = GainBudget::default();
        let mut agg = WarningAggregator::new();
        let mut hints: Vec<&'static str> = Vec::new();
        process_eq(pcm, 1, 48_000.0, &preset(eq, sat), &mut budget, &mut agg, 0, &[], &mut hints);
        hints
    }

    #[test]
    fn test_eq_silence_passthrough() {
        // Silence in → silence out for all EqCurve variants.
        for eq in [EqCurve::Flat, EqCurve::Air, EqCurve::Warmth, EqCurve::Presence] {
            let mut pcm = vec![0.0f32; 512];
            run(&mut pcm, eq, SaturationStyle::None);
            for s in &pcm {
                assert_eq!(*s, 0.0, "silence must pass through EQ ({eq:?}): got {s}");
            }
        }
    }

    #[test]
    fn test_eq_output_bounded() {
        // Any input → output in [-1.0, 1.0] and finite.
        let mut pcm: Vec<f32> = (0..512)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0))
            .collect();
        run(&mut pcm, EqCurve::Air, SaturationStyle::Tape);
        for s in &pcm {
            assert!(s.is_finite(), "output must be finite");
            assert!(*s >= -1.0 && *s <= 1.0, "output must be clamped: {s}");
        }
    }

    #[test]
    fn test_eq_hpf_attenuates_dc() {
        // DC input → near-silence after HPF (Stage 3.1).
        let mut pcm = vec![1.0f32; 4096];
        run(&mut pcm, EqCurve::Flat, SaturationStyle::None);
        // Last sample should be heavily attenuated
        let tail = pcm[4000..].iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(tail < 0.05, "HPF must attenuate DC; tail energy = {tail}");
    }

    #[test]
    fn test_eq_flat_no_budget_change() {
        // Flat curve + no Tape/Tube → only HPF runs → budget stays 0.
        let mut pcm: Vec<f32> = (0..512)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0))
            .collect();
        let mut budget = GainBudget::default();
        let mut agg = WarningAggregator::new();
        let mut hints: Vec<&'static str> = Vec::new();
        process_eq(&mut pcm, 1, 48_000.0,
            &preset(EqCurve::Flat, SaturationStyle::None),
            &mut budget, &mut agg, 0, &[], &mut hints);
        assert_eq!(budget.allocated_stage3.0, 0.0,
            "Flat/None must not consume GainBudget");
        assert!(hints.is_empty(), "no InsightHint expected for Flat/None");
    }

    #[test]
    fn test_eq_air_consumes_budget() {
        // Air curve → should request Decibels(1.5) from budget.
        let mut pcm: Vec<f32> = (0..512)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0))
            .collect();
        let mut budget = GainBudget::default();
        let mut agg = WarningAggregator::new();
        let mut hints: Vec<&'static str> = Vec::new();
        process_eq(&mut pcm, 1, 48_000.0,
            &preset(EqCurve::Air, SaturationStyle::None),
            &mut budget, &mut agg, 0, &[], &mut hints);
        assert!(budget.allocated_stage3.0 > 0.0, "Air curve must consume GainBudget");
    }

    #[test]
    fn test_eq_insight_hint_on_budget_cap() {
        // Exhaust the TOTAL budget across stage4+stage5 so request_stage3 in §3.4
        // returns 0.0 and the InsightHint fires.
        let mut pcm: Vec<f32> = (0..128)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0))
            .collect();
        let mut budget = GainBudget::default();
        // Exhaust total: stage4(4dB) + stage5(2dB) = 6dB total → 0 remaining
        budget.request_stage4(Decibels(4.0));
        budget.request_stage5(Decibels(2.0));
        assert!(budget.is_exceeded(), "budget must be fully exhausted before test");
        let mut agg = WarningAggregator::new();
        let mut hints: Vec<&'static str> = Vec::new();
        process_eq(&mut pcm, 1, 48_000.0,
            &preset(EqCurve::Flat, SaturationStyle::Tape),
            &mut budget, &mut agg, 0, &[], &mut hints);
        assert!(
            hints.contains(&INSIGHT_AIR_BAND_CAPPED),
            "exhausted total budget + Tape must emit InsightHint; hints: {hints:?}"
        );
    }
}
