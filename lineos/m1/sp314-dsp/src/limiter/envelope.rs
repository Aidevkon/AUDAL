// src/limiter/envelope.rs
// Peak follower with instantaneous attack and EMA release.
// Operates in LINEAR domain (not dB).
// Returns gain_reduction in LINEAR [0.0, 1.0].

pub struct PeakFollower {
    envelope:      f32,   // current peak envelope (linear)
    release_coeff: f32,   // EMA release coefficient
    ceiling:       f32,   // hard ceiling in linear (e.g. 0.9441 for -0.5dBFS)
}

pub const DEFAULT_CEILING_LINEAR: f32 = 0.9441_f32;  // -0.5 dBFS
pub const DECAY_FLOOR_DB:         f32 = 0.01_f32;    // snap to 1.0 below this

impl PeakFollower {
    /// release_ms: time for gain to recover after a peak (typical: 50-200ms)
    pub fn new(release_ms: f32, ceiling_linear: f32, sample_rate: u32) -> Self {
        Self {
            envelope:      0.0_f32,
            // Industry standard 10-90% formula (same as compressor)
            release_coeff: 1.0_f32 - libm::expf(
                -2.2_f32 / (release_ms * 0.001_f32 * sample_rate as f32)
            ),
            ceiling:       ceiling_linear,
        }
    }

    /// Process the true peak (max of current input AND all delayed samples).
    /// Returns gain_reduction in linear [0.0, 1.0].
    /// 1.0 = no reduction. 0.0 = full mute.
    ///
    /// true_peak must include max_abs() of both delay lines —
    /// this prevents EMA release while a peak is still in the buffer.
    #[inline]
    pub fn process(&mut self, true_peak: f32) -> f32 {
        // Instantaneous attack (0ms): envelope follows peak immediately
        if true_peak > self.envelope {
            self.envelope = true_peak;
        } else {
            // EMA release decays towards true_peak (never below it!)
            // Release only begins when peak has fully exited the delay line.
            self.envelope += (true_peak - self.envelope) * self.release_coeff;
        }

        // Anti-denormal: flush subnormal envelope values
        if libm::fabsf(self.envelope) < 1e-15_f32 {
            self.envelope = 0.0_f32;
        }

        // Compute gain reduction
        if self.envelope <= self.ceiling {
            // Below ceiling — no reduction needed
            1.0_f32
        } else {
            let gr = self.ceiling / self.envelope;

            // Decay floor: if gain reduction is negligible, snap to bypass
            // Prevents asymptotic EMA from causing pumping on quiet sections
            // and prevents denormal float values in downstream processing
            let gr_db = 20.0_f32 * libm::log10f(gr);
            if gr_db > -DECAY_FLOOR_DB {
                1.0_f32  // snap to bypass
            } else {
                gr
            }
        }
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0_f32;
    }
}
