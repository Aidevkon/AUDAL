// src/glider.rs
// Smooth parameter transition — eliminates zipper noise on knob changes.
// Runs per-sample inside process_block(). Zero allocation. No libm needed.

#[derive(Clone, Debug)]
pub struct ParameterGlider {
    current: f32,
    target: f32,
    step: f32, // precomputed: (target - current) / glide_samples
    samples_left: usize,
    glide_samples: usize, // total glide duration in samples
    sample_rate: f32,
}

impl ParameterGlider {
    /// Create a new glider at initial value.
    /// glide_ms: glide duration in milliseconds (default 300ms)
    /// sample_rate: needed to convert ms → samples
    pub fn new(initial: f32, glide_ms: f32, sample_rate: f32) -> Self {
        let glide_samples = (glide_ms / 1000.0 * sample_rate) as usize;
        Self {
            current: initial,
            target: initial,
            step: 0.0,
            samples_left: 0,
            glide_samples,
            sample_rate,
        }
    }

    /// Set a new glide duration
    pub fn set_glide_ms(&mut self, glide_ms: f32) {
        self.glide_samples = (glide_ms / 1000.0 * self.sample_rate) as usize;
    }

    /// Set a new target value. Starts gliding immediately.
    /// Called from set_node_parameter() — between blocks, never per-sample.
    pub fn set_target(&mut self, target: f32) {
        if (target - self.target).abs() < 1e-6 {
            return; // already gliding to this target — don't reset the clock
        }
        if (target - self.current).abs() < 1e-6 {
            self.current = target;
            self.target = target;
            self.samples_left = 0;
            return; // already there — no glide needed
        }

        self.target = target;

        if self.glide_samples == 0 {
            self.current = target;
            self.samples_left = 0;
            self.step = 0.0;
        } else {
            self.samples_left = self.glide_samples;
            self.step = (target - self.current) / self.glide_samples as f32;
        }
    }

    /// Set a target value and snap to it instantly without gliding.
    pub fn set_target_instant(&mut self, target: f32) {
        self.current = target;
        self.target = target;
        self.samples_left = 0;
        self.step = 0.0;
    }

    /// Advance one sample. Returns current interpolated value.
    /// Called once per sample inside process_block().
    /// Zero allocation. Pure arithmetic — no libm.
    #[inline(always)]
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f32 {
        if self.samples_left == 0 {
            return self.current;
        }
        self.current += self.step;
        self.samples_left -= 1;
        if self.samples_left == 0 {
            self.current = self.target; // snap to exact target at end
        }
        self.current
    }

    /// True if glide is in progress.
    pub fn is_gliding(&self) -> bool {
        self.samples_left > 0
    }

    /// Current value (without advancing).
    pub fn value(&self) -> f32 {
        self.current
    }

    pub fn reset(&mut self) {
        self.current = self.target;
        self.samples_left = 0;
        self.step = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repeated_set_target_does_not_reset_glide_clock() {
        // Regression test for the Zeno's-Paradox bug found 2026-07-10:
        // calling set_target() repeatedly with the SAME target every
        // block was resetting the glide clock each time, turning a
        // linear ramp into an asymptotic curve that never reached target.
        let sample_rate = 48000.0;
        let glide_ms = 300.0;
        let mut glider = ParameterGlider::new(1.0, glide_ms, sample_rate);

        glider.set_target(0.501);

        // Simulate the buggy pattern: call set_target with the SAME value
        // on every "block" for the full glide duration.
        let glide_samples = (glide_ms / 1000.0 * sample_rate) as usize;
        for _ in 0..glide_samples {
            glider.set_target(0.501); // same target, called every sample/block
            glider.next();
        }

        // After the full glide duration, current MUST have reached target
        // exactly, despite the repeated same-target calls.
        assert!(
            (glider.value() - 0.501).abs() < 1e-4,
            "glide should reach target even with repeated same-target calls, got {}",
            glider.value()
        );
    }
}
