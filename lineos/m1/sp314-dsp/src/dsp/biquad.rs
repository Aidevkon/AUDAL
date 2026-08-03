#[derive(Clone, Copy)]
pub struct Biquad {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
    pub w1: f32,
    pub w2: f32,
}

impl Biquad {
    pub fn process(&mut self, x: f32) -> f32 {
        let w0 = x - self.a1 * self.w1 - self.a2 * self.w2;
        let y = self.b0 * w0 + self.b1 * self.w1 + self.b2 * self.w2;
        self.w2 = self.w1;
        self.w1 = w0;
        y
    }
}

pub fn butter_lp2_prewarped(freq: f32, sr: f32) -> Biquad {
    let k = libm::tanf(core::f32::consts::PI * freq / sr);
    let q = 1.0_f32 / core::f32::consts::SQRT_2;
    let norm = 1.0 / (k * k + k / q + 1.0);
    let a1 = 2.0 * (k * k - 1.0) * norm;
    let a2 = (k * k - k / q + 1.0) * norm;
    let b0 = k * k * norm;
    let b1 = 2.0 * b0;
    let b2 = b0;
    Biquad {
        b0, b1, b2, a1, a2,
        w1: 0.0, w2: 0.0,
    }
}

pub fn butter_hp2_prewarped(freq: f32, sr: f32) -> Biquad {
    let k = libm::tanf(core::f32::consts::PI * freq / sr);
    let q = 1.0_f32 / core::f32::consts::SQRT_2;
    let norm = 1.0 / (k * k + k / q + 1.0);
    let a1 = 2.0 * (k * k - 1.0) * norm;
    let a2 = (k * k - k / q + 1.0) * norm;
    let b0 = norm;
    let b1 = -2.0 * b0;
    let b2 = b0;
    Biquad {
        b0, b1, b2, a1, a2,
        w1: 0.0, w2: 0.0,
    }
}
