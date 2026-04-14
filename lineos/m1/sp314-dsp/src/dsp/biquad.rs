//! Biquad filter — second-order IIR filter.
//! Ported from sm-core/src/dsp/biquad.rs.
//! Authority: LineOS Constitution v2.0 §09.1 — libm only, no std::f32 methods.
//!
//! Supports: Low-pass, High-pass, High-shelf, Peaking EQ.
//! All float math uses libm. No std::f32 / std::f64 methods permitted.

use core::f32::consts::PI;

/// Direct Form II transposed biquad filter.
#[derive(Debug, Clone)]
pub struct Biquad {
    // Coefficients
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // State
    s1: f32,
    s2: f32,
}

impl Biquad {
    pub fn new() -> Self {
        Self {
            b0: 1.0, b1: 0.0, b2: 0.0,
            a1: 0.0, a2: 0.0,
            s1: 0.0, s2: 0.0,
        }
    }

    /// High-pass filter at `freq_hz` with given Q.
    pub fn set_hpf(&mut self, freq_hz: f32, sample_rate: f32, q: f32) {
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let cos_w0 = libm::cosf(w0);
        let sin_w0 = libm::sinf(w0);
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        self.set_coeffs(b0, b1, b2, a0, a1, a2);
    }

    /// Low-pass filter at `freq_hz` with given Q.
    pub fn set_lpf(&mut self, freq_hz: f32, sample_rate: f32, q: f32) {
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let cos_w0 = libm::cosf(w0);
        let sin_w0 = libm::sinf(w0);
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        self.set_coeffs(b0, b1, b2, a0, a1, a2);
    }

    /// High-shelf filter at `freq_hz` with given gain_db and Q.
    pub fn set_high_shelf(&mut self, freq_hz: f32, sample_rate: f32, gain_db: f32, q: f32) {
        let a = libm::powf(10.0, gain_db / 40.0);
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let cos_w0 = libm::cosf(w0);
        let sin_w0 = libm::sinf(w0);
        let alpha = sin_w0 / (2.0 * q);

        let sq_a = libm::sqrtf(a);

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * sq_a * alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * sq_a * alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * sq_a * alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * sq_a * alpha;

        self.set_coeffs(b0, b1, b2, a0, a1, a2);
    }

    /// Peaking EQ at `freq_hz` with given gain_db and bandwidth_octaves.
    pub fn set_peaking(&mut self, freq_hz: f32, sample_rate: f32, gain_db: f32, bw_octaves: f32) {
        let a = libm::powf(10.0, gain_db / 40.0);
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let cos_w0 = libm::cosf(w0);
        let sin_w0 = libm::sinf(w0);
        let alpha = sin_w0 * libm::sinhf(
            libm::logf(2.0) / 2.0 * bw_octaves * w0 / sin_w0,
        );

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        self.set_coeffs(b0, b1, b2, a0, a1, a2);
    }

    fn set_coeffs(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
        // Reset state on coefficient change
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    /// Process a single sample through the filter (Direct Form II transposed).
    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biquad_hpf_silence_passthrough() {
        let mut f = Biquad::new();
        f.set_hpf(30.0, 48000.0, 0.707);
        let out = f.process(0.0);
        assert!(out.abs() < 1e-9);
    }

    #[test]
    fn biquad_hpf_attenuates_dc() {
        let mut f = Biquad::new();
        f.set_hpf(100.0, 48000.0, 0.707);
        // Feed DC signal, expect heavy attenuation
        let mut out = 0.0;
        for _ in 0..4096 {
            out = f.process(1.0);
        }
        // HPF should attenuate DC significantly
        assert!(out.abs() < 0.1, "HPF should attenuate DC, got {out}");
    }

    #[test]
    fn biquad_lpf_attenuates_high_freq() {
        let mut f = Biquad::new();
        f.set_lpf(100.0, 48000.0, 0.707);
        // Feed high-frequency signal, expect heavy attenuation
        let mut out = 0.0;
        for i in 0..4096 {
            let t = i as f32 / 48000.0;
            let x = libm::sinf(2.0 * core::f32::consts::PI * 10000.0 * t);
            out = f.process(x);
        }
        assert!(out.abs() < 0.01, "LPF should attenuate HF, got {out}");
    }
}
