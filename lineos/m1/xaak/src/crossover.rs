//! Duplicated from sp314_dsp::compressor::crossover::CrossoverLR4
//! xaak has no dependency on sp314-dsp by design (see commit 15542a3).
//! Math is fixed Linkwitz-Riley 4th order, low duplication risk.

pub struct CrossoverLR4 {
    lp_coeffs: [f32; 5],
    hp_coeffs: [f32; 5],
    lp_state: [[f32; 2]; 2],
    hp_state: [[f32; 2]; 2],
}

impl CrossoverLR4 {
    pub fn new(crossover_hz: f32, sample_rate: u32) -> Self {
        let w0 = 2.0 * core::f32::consts::PI * crossover_hz / sample_rate as f32;
        let q = 1.0 / 2.0f32.sqrt();
        let cos_w = w0.cos();
        let sin_w = w0.sin();
        let alpha = sin_w / (2.0 * q);

        let lp_b0 = (1.0 - cos_w) / 2.0;
        let lp_b1 = 1.0 - cos_w;
        let lp_b2 = (1.0 - cos_w) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w;
        let a2 = 1.0 - alpha;

        let lp_coeffs = [lp_b0 / a0, lp_b1 / a0, lp_b2 / a0, a1 / a0, a2 / a0];

        let hp_b0 = (1.0 + cos_w) / 2.0;
        let hp_b1 = -(1.0 + cos_w);
        let hp_b2 = (1.0 + cos_w) / 2.0;

        let hp_coeffs = [hp_b0 / a0, hp_b1 / a0, hp_b2 / a0, a1 / a0, a2 / a0];

        Self {
            lp_coeffs,
            hp_coeffs,
            lp_state: [[0.0; 2]; 2],
            hp_state: [[0.0; 2]; 2],
        }
    }

    #[inline]
    fn process_biquad(x: f32, coeffs: &[f32; 5], state: &mut [f32; 2]) -> f32 {
        let [b0, b1, b2, a1, a2] = *coeffs;
        let [w1, w2] = *state;

        let y = b0 * x + w1;
        state[0] = b1 * x - a1 * y + w2;
        state[1] = b2 * x - a2 * y;

        if state[0].abs() < 1e-15 {
            state[0] = 0.0;
        }
        if state[1].abs() < 1e-15 {
            state[1] = 0.0;
        }

        y
    }

    pub fn process(&mut self, x: f32) -> (f32, f32) {
        let lp1 = Self::process_biquad(x, &self.lp_coeffs, &mut self.lp_state[0]);
        let low = Self::process_biquad(lp1, &self.lp_coeffs, &mut self.lp_state[1]);

        let hp1 = Self::process_biquad(x, &self.hp_coeffs, &mut self.hp_state[0]);
        let high = Self::process_biquad(hp1, &self.hp_coeffs, &mut self.hp_state[1]);

        (low, high)
    }

    pub fn reset(&mut self) {
        self.lp_state = [[0.0; 2]; 2];
        self.hp_state = [[0.0; 2]; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_freq_passes_through_low_output_mostly() {
        let sr = 48000;
        let crossover_hz = 1000.0;
        let mut xover = CrossoverLR4::new(crossover_hz, sr);
        let freq = 100.0; // well below crossover
        let mut sum_low_sq = 0.0_f32;
        let mut sum_high_sq = 0.0_f32;
        let n = 4800; // 100ms, enough to settle past filter transient
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let x = (2.0 * std::f32::consts::PI * freq * t).sin();
            let (low, high) = xover.process(x);
            if i > 1000 { // skip transient
                sum_low_sq += low * low;
                sum_high_sq += high * high;
            }
        }
        // Low-frequency signal should dominate the low output, be negligible in high
        assert!(sum_low_sq > sum_high_sq * 10.0,
            "100Hz tone (crossover=1000Hz): low_energy={} should dominate high_energy={}",
            sum_low_sq, sum_high_sq);
    }

    #[test]
    fn high_freq_passes_through_high_output_mostly() {
        let sr = 48000;
        let crossover_hz = 1000.0;
        let mut xover = CrossoverLR4::new(crossover_hz, sr);
        let freq = 10000.0; // well above crossover
        let mut sum_low_sq = 0.0_f32;
        let mut sum_high_sq = 0.0_f32;
        let n = 4800;
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let x = (2.0 * std::f32::consts::PI * freq * t).sin();
            let (low, high) = xover.process(x);
            if i > 1000 {
                sum_low_sq += low * low;
                sum_high_sq += high * high;
            }
        }
        assert!(sum_high_sq > sum_low_sq * 10.0,
            "10kHz tone (crossover=1000Hz): high_energy={} should dominate low_energy={}",
            sum_high_sq, sum_low_sq);
    }
}
