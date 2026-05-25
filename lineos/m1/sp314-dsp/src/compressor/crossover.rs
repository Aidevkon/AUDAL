pub struct CrossoverLR4 {
    lp_coeffs: [f32; 5],
    hp_coeffs: [f32; 5],
    lp_state: [[f32; 2]; 2],
    hp_state: [[f32; 2]; 2],
}

impl CrossoverLR4 {
    pub fn new(crossover_hz: f32, sample_rate: u32) -> Self {
        let w0 = 2.0 * core::f32::consts::PI * crossover_hz / sample_rate as f32;
        let q = 1.0 / libm::sqrtf(2.0);
        let cos_w = libm::cosf(w0);
        let sin_w = libm::sinf(w0);
        let alpha = sin_w / (2.0 * q);

        let lp_b0 = (1.0 - cos_w) / 2.0;
        let lp_b1 = 1.0 - cos_w;
        let lp_b2 = (1.0 - cos_w) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w;
        let a2 = 1.0 - alpha;

        let lp_coeffs = [lp_b0/a0, lp_b1/a0, lp_b2/a0, a1/a0, a2/a0];

        let hp_b0 = (1.0 + cos_w) / 2.0;
        let hp_b1 = -(1.0 + cos_w);
        let hp_b2 = (1.0 + cos_w) / 2.0;
        
        let hp_coeffs = [hp_b0/a0, hp_b1/a0, hp_b2/a0, a1/a0, a2/a0];

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

        if libm::fabsf(state[0]) < 1e-15 { state[0] = 0.0; }
        if libm::fabsf(state[1]) < 1e-15 { state[1] = 0.0; }

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
