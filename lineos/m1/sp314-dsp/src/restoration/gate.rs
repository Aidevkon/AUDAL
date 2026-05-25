// src/restoration/gate.rs
// Noise Gate — downward expander.
// Constitutional: libm only, zero allocation, precomputed coefficients.

use libm::{expf, fabsf};

/// Noise Gate — downward expander to silence background noise.
/// Features a linked stereo detector, hold time to prevent chatter,
/// and smooth exponential attack/release curves.
pub struct NoiseGate {
    threshold_linear: f32,   // -45 dBFS default
    attack_coef:  f32,       // precomputed: 1ms
    release_coef: f32,       // precomputed: 100ms
    hold_samples: usize,     // precomputed: 50ms hold before closing
    hold_counter: usize,
    gain:         f32,       // current gate gain [0.0, 1.0]
    enabled:      bool,
}

impl NoiseGate {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            // -45 dBFS threshold — below this, gate closes
            threshold_linear: libm::powf(10.0, -45.0 / 20.0),
            attack_coef:  expf(-1.0 / (sample_rate * 0.001)),  // 1ms open
            release_coef: expf(-1.0 / (sample_rate * 0.100)),  // 100ms close
            hold_samples: (sample_rate * 0.050) as usize,      // 50ms hold
            hold_counter: 0,
            gain:         1.0,
            enabled:      true,
        }
    }

    #[inline]
    pub fn process_stereo(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled {
            return (left, right);
        }

        // Use max of L/R for detection (linked stereo gate)
        let level = fabsf(left).max(fabsf(right));

        let target_gain = if level >= self.threshold_linear {
            self.hold_counter = self.hold_samples; // reset hold
            1.0_f32
        } else if self.hold_counter > 0 {
            self.hold_counter -= 1;
            1.0_f32  // still open during hold
        } else {
            0.0_f32  // close
        };

        // Smooth gain changes — attack when opening, release when closing
        let coef = if target_gain > self.gain {
            self.attack_coef
        } else {
            self.release_coef
        };

        self.gain = self.gain + (target_gain - self.gain) * (1.0 - coef);

        (left * self.gain, right * self.gain)
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
        self.hold_counter = 0;
    }
}
