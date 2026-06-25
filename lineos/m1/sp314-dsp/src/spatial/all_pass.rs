// sp314-dsp/src/spatial/all_pass.rs

/// 2nd Order All-Pass Filter (RBJ Cookbook)
/// Passes all frequencies at equal amplitude (gain = 1.0) but alters the phase.
/// Phase shift is exactly -180 degrees at `freq_hz`.
#[derive(Debug, Clone)]
pub struct AllPassFilter {
    // Coefficients
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,

    // State (Direct Form 1)
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl AllPassFilter {
    /// Create a new 2nd order all-pass filter.
    pub fn new(freq_hz: f32, q: f32, sample_rate: u32) -> Self {
        let mut filter = Self {
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        };
        filter.update_coefficients(freq_hz, q, sample_rate);
        filter
    }

    /// Update filter coefficients without clearing the state.
    pub fn update_coefficients(&mut self, freq_hz: f32, q: f32, sample_rate: u32) {
        let w0 = 2.0 * core::f32::consts::PI * freq_hz / sample_rate as f32;
        let alpha = w0.sin() / (2.0 * q);

        let b0 = 1.0 - alpha;
        let b1 = -2.0 * w0.cos();
        let b2 = 1.0 + alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * w0.cos();
        let a2 = 1.0 - alpha;

        // Normalize
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// Process a single sample through the filter.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;

        // Shift state
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;

        y
    }

    /// Reset internal state delays
    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allpass_preserves_magnitude_shifts_phase() {
        // Feed a steady-state sine, confirm OUTPUT AMPLITUDE matches INPUT AMPLITUDE
        // (within tight tolerance) after the filter settles — proving zero gain
        // change, only phase shift. This is the core all-pass guarantee.
        let sr = 48000;
        let mut filt = AllPassFilter::new(1000.0, 0.707, sr);
        let freq = 1000.0;
        let n = 4800;
        let mut max_in = 0.0_f32;
        let mut max_out = 0.0_f32;
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let x = (2.0 * std::f32::consts::PI * freq * t).sin();
            let y = filt.process(x);
            if i > 1000 {
                // skip transient
                max_in = max_in.max(x.abs());
                max_out = max_out.max(y.abs());
            }
        }
        assert!(
            (max_in - max_out).abs() < 0.01,
            "All-pass must preserve magnitude: in_peak={}, out_peak={}",
            max_in,
            max_out
        );
    }
}
