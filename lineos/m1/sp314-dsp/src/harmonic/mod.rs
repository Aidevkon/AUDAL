// src/harmonic/mod.rs

/// Configuration for the Harmonic Exciter.
/// Controls the balance between even (tape) and odd (tube) harmonics.
#[derive(Clone, Debug, Copy)]
pub struct HarmonicConfig {
    pub drive: f32,
    pub even_amount: f32,
    pub odd_amount: f32,
    pub mix: f32,
}

impl Default for HarmonicConfig {
    fn default() -> Self {
        Self {
            drive: 2.0,
            even_amount: 0.6,
            odd_amount: 0.2,
            mix: 0.3,
        }
    }
}

/// Harmonic Exciter Engine.
/// Adds warmth and presence using zero-allocation, stateless libm-based waveshaping.
pub struct HarmonicEngine {
    config: HarmonicConfig,
}

impl HarmonicEngine {
    pub fn new(config: HarmonicConfig) -> Self {
        Self { config }
    }

    #[inline]
    fn process_sample(&self, x: f32) -> f32 {
        let drive = self.config.drive;
        let even_amt = self.config.even_amount;
        let odd_amt = self.config.odd_amount;
        let mix = self.config.mix;

        // Odd harmonics (tube/presence) — symmetric soft clip
        let y_odd = libm::tanhf(drive * x);

        // Even harmonics (tape/warmth) — asymmetric signed squaring
        let y_even = x + (x * libm::fabsf(x)) * 0.5;

        // Combine
        let wet = (even_amt * y_even) + (odd_amt * y_odd);
        
        // Final soft-clip
        let wet_clamped = libm::tanhf(wet);

        // Mix
        x * (1.0 - mix) + wet_clamped * mix
    }

    #[inline]
    pub fn process_frame(&mut self, mid: f32, side: f32) -> (f32, f32) {
        if self.config.mix == 0.0 {
            return (mid, side);
        }
        (self.process_sample(mid), self.process_sample(side))
    }

    pub fn reset(&mut self) {
        // No state
    }
}
