//! Stage 5.5 — Pre-Limiter Safety Net — sp314-dsp v2.9 §Stage 5.5.
//!
//! Always active. Runs after Stage 5 saturation, before Stage 6 limiting.
//!
//! **5.5.1** Soft clipper: threshold -1.5 dBFS, knee 0.5.
//! **5.5.2** If gain reduction > 0.5 dB applied:
//!   - Emit `PipelineWarning::LimiterOverwork { source: PreLimiterSafety }`
//!   - Return `bypass_flag = true` for this block
//!
//! # Returns
//! `bypass_flag: bool` — true if GR > 0.5 dB was applied. The caller
//! (Stage 6, when wired) uses this to reduce the lookahead contribution
//! for frames where pre-limiter clipping was significant.
//!
//! # Determinism
//! All math: libm only. finalize_sample() at output boundary.

use crate::pipeline::math::finalize_sample;
use crate::pipeline::warnings::{LimiterOverworkSource, PipelineWarning, WarningAggregator};
use crate::types::units::Decibels;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Soft clip threshold: -1.5 dBFS
const THRESHOLD_DB:   f32 = -1.5;
/// Soft knee width
const KNEE_WIDTH:     f32 = 0.5;
/// GR threshold for bypass_flag: if > 0.5 dB reduction applied
const GR_BYPASS_DB:   f32 = 0.5;

// ── process_prelimit ─────────────────────────────────────────────────────────

/// Process `pcm` in-place through Stage 5.5 pre-limiter safety net.
///
/// Returns `bypass_flag`:
/// - `false` — no significant GR applied; pipeline continues normally
/// - `true`  — GR > 0.5 dB applied; `LimiterOverwork::PreLimiterSafety` emitted
pub fn process_prelimit(
    pcm:         &mut [f32],
    aggregator:  &mut WarningAggregator,
    block_index: u64,
) -> bool {
    if pcm.is_empty() { return false; }

    let threshold_lin = Decibels(THRESHOLD_DB).to_linear().0; // ≈ 0.8414 linear
    let knee_start    = Decibels(THRESHOLD_DB - KNEE_WIDTH * 0.5).to_linear().0;
    let knee_end      = threshold_lin;  // knee_end == threshold for soft knee

    let mut max_gr_db: f32 = 0.0;

    for sample in pcm.iter_mut() {
        let x   = *sample;
        let ax  = libm::fabsf(x);

        let clipped = if ax <= knee_start {
            x
        } else if ax <= knee_end {
            // Soft knee zone: smooth transition
            let sgn = if x >= 0.0 { 1.0 } else { -1.0 };
            let t   = (ax - knee_start) / KNEE_WIDTH; // 0..1 in knee
            let y   = knee_start + KNEE_WIDTH * libm::tanhf(t * 2.0) * 0.5;
            sgn * y
        } else {
            // Hard limit with soft asymptote above threshold
            let sgn = if x >= 0.0 { 1.0 } else { -1.0 };
            let excess = ax - threshold_lin;
            sgn * (threshold_lin + excess * libm::tanhf(excess / threshold_lin) * 0.2)
        };

        // Track maximum gain reduction applied
        if ax > 1e-9 {
            let out_abs = libm::fabsf(clipped);
            if out_abs < ax {
                // GR in dB: 20 * log10(out/in) — negative value
                let gr = 20.0 * libm::log10f(out_abs / ax); // negative
                let gr_abs = libm::fabsf(gr);
                if gr_abs > max_gr_db { max_gr_db = gr_abs; }
            }
        }

        *sample = finalize_sample(clipped);
    }

    // §5.5.2 bypass_flag if GR > 0.5 dB
    let bypass = max_gr_db > GR_BYPASS_DB;
    if bypass {
        aggregator.push(
            PipelineWarning::LimiterOverwork { source: LimiterOverworkSource::PreLimiterSafety },
            block_index,
        );
    }
    bypass
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::warnings::WarningAggregator;

    #[test]
    fn test_prelimit_silence() {
        let mut pcm = alloc::vec![0.0f32; 256];
        let mut agg = WarningAggregator::new();
        let bypass = process_prelimit(&mut pcm, &mut agg, 0);
        for s in &pcm { assert_eq!(*s, 0.0); }
        assert!(!bypass, "silence must not trigger bypass");
    }

    #[test]
    fn test_prelimit_quiet_signal_passthrough() {
        // -6 dBFS signal must pass unchanged (below threshold of -1.5 dBFS)
        let amp = Decibels(-6.0).to_linear().0; // ≈ 0.501
        let mut pcm: alloc::vec::Vec<f32> = (0..256)
            .map(|i| amp * libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0))
            .collect();
        let original = pcm.clone();
        let mut agg = WarningAggregator::new();
        let bypass = process_prelimit(&mut pcm, &mut agg, 0);
        // All samples should be unchanged (within finalize_sample quantization)
        for (orig, clipped) in original.iter().zip(pcm.iter()) {
            assert!(libm::fabsf(clipped - orig) < 1e-5,
                "quiet signal must pass unchanged: orig={orig}, clipped={clipped}");
        }
        assert!(!bypass, "quiet signal must not trigger bypass");
    }

    #[test]
    fn test_prelimit_loud_signal_clips() {
        // +3 dBFS signal must be clipped and trigger bypass
        let amp = 1.4f32; // well above threshold
        let mut pcm: alloc::vec::Vec<f32> = (0..256)
            .map(|i| amp * libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0))
            .collect();
        let mut agg = WarningAggregator::new();
        let bypass = process_prelimit(&mut pcm, &mut agg, 0);
        // All output must be in [-1, 1] and finite
        for s in &pcm {
            assert!(s.is_finite() && *s >= -1.0 && *s <= 1.0,
                "clipped output must be in [-1,1]: {s}");
        }
        assert!(bypass, "loud signal must trigger bypass_flag");
    }

    #[test]
    fn test_prelimit_bypass_emits_warning() {
        let mut pcm = alloc::vec![1.4f32; 64];
        let mut agg = WarningAggregator::new();
        let bypass = process_prelimit(&mut pcm, &mut agg, 42);
        assert!(bypass);
        let has_warn = agg.records().iter().any(|r| {
            r.warning == PipelineWarning::LimiterOverwork {
                source: LimiterOverworkSource::PreLimiterSafety,
            }
        });
        assert!(has_warn, "bypass must emit LimiterOverwork::PreLimiterSafety");
    }

    #[test]
    fn test_prelimit_output_bounded() {
        // Any finite input must produce output in [-1, 1]
        let mut pcm: alloc::vec::Vec<f32> = (0..512)
            .map(|i| 2.0 * libm::sinf(2.0 * core::f32::consts::PI * 1000.0 * i as f32 / 48_000.0))
            .collect();
        let mut agg = WarningAggregator::new();
        process_prelimit(&mut pcm, &mut agg, 0);
        for s in &pcm {
            assert!(s.is_finite() && *s >= -1.0 && *s <= 1.0,
                "output must be bounded: {s}");
        }
    }
}
