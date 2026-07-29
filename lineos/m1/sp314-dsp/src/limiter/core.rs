// src/limiter/core.rs
// Orchestrates RingBuffer + PeakFollower for stereo lookahead limiting.

use crate::limiter::delay::{PeakRing, RingBuffer};
use crate::limiter::envelope::PeakFollower;
use crate::limiter::midside::MidSideProcessor;
use crate::limiter::true_peak::TruePeakDetector;

pub struct BrickwallLimiter {
    delay_l: RingBuffer,
    delay_r: RingBuffer,
    /// F-048: peak estimates aligned with the audio delay line.
    peak_ring: PeakRing,
    follower: PeakFollower,
    lookahead: usize,
    midside: MidSideProcessor,
    midside_eq_enabled: bool,
    true_peak: TruePeakDetector,
    true_peak_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimiterConfig {
    pub release_ms: f32, // default: 100.0
    /// Slow blend release ms.
    /// Lower = more transient punch.
    /// Default 30ms (was hardcoded 100ms).
    pub blend_release_ms: f32,
    pub ceiling_db: f32,          // default: -0.5
    pub midside_eq_enabled: bool, // default: false
    pub true_peak_enabled: bool,  // default: true
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self {
            release_ms: 100.0_f32,
            blend_release_ms: 30.0,
            ceiling_db: -0.5_f32,
            midside_eq_enabled: false,
            true_peak_enabled: true,
        }
    }
}

/// F-048: the 4x polyphase estimator samples the interpolated waveform at four
/// points per period; the real crest almost never lands on one of them, so it
/// reads low by a fixed amount. Measured across seven signals, three release
/// times and four ceilings: 0.2526 to 0.2877 dB, invariant to release, to gain
/// reduction and to the ceiling itself — a property of the estimator, not of the
/// follower. The follower therefore aims this far below the requested ceiling.
/// 0.35 = worst measured 0.2877 plus margin, still inside BS.1770's 0.5-1 dB
/// allowance for 4x measurement. Raising the oversampling factor is the real
/// fix and would let this shrink.
pub const TRUE_PEAK_HEADROOM_DB: f32 = 0.35;

impl BrickwallLimiter {
    pub fn new(config: LimiterConfig, sample_rate: u32) -> Self {
        // F-048: aim below the requested ceiling to absorb the estimator's
        // fixed underread. See TRUE_PEAK_HEADROOM_DB.
        let target_db = if config.true_peak_enabled {
            config.ceiling_db - TRUE_PEAK_HEADROOM_DB
        } else {
            config.ceiling_db
        };
        let ceiling_linear = libm::powf(10.0_f32, target_db / 20.0_f32);
        let lookahead = (sample_rate as f32 * 0.005).round() as usize; // 5ms dynamic
        Self {
            delay_l: RingBuffer::new(lookahead),
            delay_r: RingBuffer::new(lookahead),
            peak_ring: PeakRing::new(lookahead),
            follower: PeakFollower::new(
                config.release_ms,
                config.blend_release_ms,
                ceiling_linear,
                sample_rate,
                lookahead,
            ),
            lookahead,
            midside: MidSideProcessor::new(),
            midside_eq_enabled: config.midside_eq_enabled,
            true_peak: TruePeakDetector::new(),
            true_peak_enabled: config.true_peak_enabled,
        }
    }

    #[inline]
    pub fn process(&mut self, left: &mut f32, right: &mut f32) {
        // Apply Mid/Side HP EQ if enabled
        // Must run BEFORE delay line and sidechain
        if self.midside_eq_enabled {
            let (l, r) = self.midside.process(*left, *right);
            *left = l;
            *right = r;
        }

        let current_peak = if self.true_peak_enabled {
            self.true_peak.process(*left, *right)
        } else {
            libm::fmaxf(libm::fabsf(*left), libm::fabsf(*right))
        };

        // F-048: read the peak estimate belonging to the samples now leaving
        // the delay line, not their raw magnitude. Reading max_abs() here made
        // the limiter sample-peak in practice: a true-peak estimate was computed
        // for each incoming sample and then thrown away, so the follower's
        // 240-sample ramp never saw a value it had to act on.
        let delayed_peak = self.peak_ring.max();
        self.peak_ring.push(current_peak);
        let sidechain_peak = libm::fmaxf(current_peak, delayed_peak);

        let gain_reduction = self.follower.process(sidechain_peak);

        let delayed_l = self.delay_l.push_and_pop(*left);
        let delayed_r = self.delay_r.push_and_pop(*right);

        *left = delayed_l * gain_reduction;
        *right = delayed_r * gain_reduction;
    }

    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        for i in 0..left.len() {
            self.process(&mut left[i], &mut right[i]);
        }
    }

    pub fn reset(&mut self) {
        self.delay_l.reset();
        self.delay_r.reset();
        self.follower.reset();
        self.midside.reset();
        self.true_peak.reset();
        self.peak_ring.reset();
    }

    pub fn lookahead_samples(&self) -> usize {
        self.lookahead
    }
}
