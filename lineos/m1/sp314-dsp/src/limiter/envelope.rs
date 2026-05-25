// src/limiter/envelope.rs
// Peak follower with linear attack and EMA release.
// Operates in LINEAR domain (not dB).
// Returns gain_reduction in LINEAR [0.0, 1.0].

pub struct PeakFollower {
    envelope:      f32,
    release_coeff: f32,
    ceiling:       f32,
    lookahead_samples: usize,
    ramp_step:     f32,
}

pub const DEFAULT_CEILING_LINEAR: f32 = 0.9441_f32;  // -0.5 dBFS
pub const DECAY_FLOOR_DB:         f32 = 0.01_f32;    // snap to 1.0 below this

impl PeakFollower {
    pub fn new(release_ms: f32, ceiling_linear: f32, sample_rate: u32, lookahead_samples: usize) -> Self {
        Self {
            envelope:      0.0_f32,
            release_coeff: 1.0_f32 - libm::expf(
                -2.2_f32 / (release_ms * 0.001_f32 * sample_rate as f32)
            ),
            ceiling:       ceiling_linear,
            lookahead_samples: lookahead_samples.max(1),
            ramp_step:     0.0_f32,
        }
    }

    #[inline]
    pub fn process(&mut self, true_peak: f32) -> f32 {
        if true_peak > self.envelope {
            let min_required_step = (true_peak - self.envelope) / self.lookahead_samples as f32;
            if min_required_step > self.ramp_step {
                self.ramp_step = min_required_step;
            }
            self.envelope += self.ramp_step;
            if self.envelope > true_peak {
                self.envelope = true_peak;
                self.ramp_step = 0.0_f32;
            }
        } else {
            self.ramp_step = 0.0_f32;
            self.envelope += (true_peak - self.envelope) * self.release_coeff;
        }

        if libm::fabsf(self.envelope) < 1e-15_f32 {
            self.envelope = 0.0_f32;
        }

        if self.envelope <= self.ceiling {
            1.0_f32
        } else {
            let gr = self.ceiling / self.envelope;
            let gr_db = 20.0_f32 * libm::log10f(gr);
            if gr_db > -DECAY_FLOOR_DB {
                1.0_f32
            } else {
                gr
            }
        }
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0_f32;
        self.ramp_step = 0.0_f32;
    }
}
