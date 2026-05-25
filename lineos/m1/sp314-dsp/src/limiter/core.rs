// src/limiter/core.rs
// Orchestrates RingBuffer + PeakFollower for stereo lookahead limiting.

use crate::limiter::delay::RingBuffer;
use crate::limiter::envelope::PeakFollower;

pub struct BrickwallLimiter {
    delay_l:  RingBuffer,
    delay_r:  RingBuffer,
    follower: PeakFollower,
    lookahead: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimiterConfig {
    pub release_ms:      f32,   // default: 100.0
    pub ceiling_db:      f32,   // default: -0.5
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self {
            release_ms:  100.0_f32,
            ceiling_db:  -0.5_f32,
        }
    }
}

impl BrickwallLimiter {
    pub fn new(config: LimiterConfig, sample_rate: u32) -> Self {
        let ceiling_linear = libm::powf(10.0_f32, config.ceiling_db / 20.0_f32);
        let lookahead = (sample_rate as f32 * 0.005).round() as usize; // 5ms dynamic
        Self {
            delay_l:  RingBuffer::new(lookahead),
            delay_r:  RingBuffer::new(lookahead),
            follower: PeakFollower::new(config.release_ms, ceiling_linear, sample_rate, lookahead),
            lookahead,
        }
    }

    #[inline]
    pub fn process(&mut self, left: &mut f32, right: &mut f32) {
        let max_delayed_l = self.delay_l.max_abs();
        let max_delayed_r = self.delay_r.max_abs();
        let delayed_peak  = libm::fmaxf(max_delayed_l, max_delayed_r);

        let current_peak = libm::fmaxf(libm::fabsf(*left), libm::fabsf(*right));
        let true_peak    = libm::fmaxf(current_peak, delayed_peak);

        let gain_reduction = self.follower.process(true_peak);

        let delayed_l = self.delay_l.push_and_pop(*left);
        let delayed_r = self.delay_r.push_and_pop(*right);

        *left  = delayed_l * gain_reduction;
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
    }

    pub fn lookahead_samples(&self) -> usize {
        self.lookahead
    }
}
