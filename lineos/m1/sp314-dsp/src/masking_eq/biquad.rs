#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterType {
    Bell,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
}

#[derive(Debug, Clone, Copy)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self::new()
    }
}

impl BiquadCoeffs {
    pub fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BiquadState {
    pub s1: f64,
    pub s2: f64,
}

impl Default for BiquadState {
    fn default() -> Self {
        Self::new()
    }
}

impl BiquadState {
    pub fn new() -> Self {
        Self { s1: 0.0, s2: 0.0 }
    }
    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}

pub fn process_tdf2(x: f32, coeffs: &BiquadCoeffs, state: &mut BiquadState) -> f32 {
    let xd = x as f64;
    let y = coeffs.b0 * xd + state.s1;
    state.s1 = coeffs.b1 * xd - coeffs.a1 * y + state.s2;
    state.s2 = coeffs.b2 * xd - coeffs.a2 * y;
    // Denormal flush
    if libm::fabs(state.s1) < 1e-30 {
        state.s1 = 0.0;
    }
    if libm::fabs(state.s2) < 1e-30 {
        state.s2 = 0.0;
    }
    y as f32
}

pub fn rbj_bell(center_hz: f64, gain_db: f64, q: f64, sample_rate: f64) -> BiquadCoeffs {
    let a = libm::pow(10.0, gain_db / 40.0);
    let w0 = 2.0 * core::f64::consts::PI * center_hz / sample_rate;
    let cos_w = libm::cos(w0);
    let sin_w = libm::sin(w0);
    let alpha = sin_w / (2.0 * q);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos_w;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - alpha / a;

    BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

pub fn rbj_bell_fast(cos_w0: f64, alpha: f64, gain_db: f64) -> BiquadCoeffs {
    let a = libm::pow(10.0_f64, gain_db / 40.0_f64);
    let a0 = 1.0_f64 + alpha / a;
    BiquadCoeffs {
        b0: (1.0_f64 + alpha * a) / a0,
        b1: (-2.0_f64 * cos_w0) / a0,
        b2: (1.0_f64 - alpha * a) / a0,
        a1: (-2.0_f64 * cos_w0) / a0,
        a2: (1.0_f64 - alpha / a) / a0,
    }
}

pub fn rbj_low_shelf(center_hz: f64, gain_db: f64, q: f64, sample_rate: f64) -> BiquadCoeffs {
    let a = libm::pow(10.0, gain_db / 40.0);
    let w0 = 2.0 * core::f64::consts::PI * center_hz / sample_rate;
    let cos_w = libm::cos(w0);
    let sin_w = libm::sin(w0);
    let alpha = sin_w / (2.0 * q);
    let sq_a = libm::sqrt(a);

    let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w + 2.0 * sq_a * alpha);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w);
    let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w - 2.0 * sq_a * alpha);
    let a0 = (a + 1.0) + (a - 1.0) * cos_w + 2.0 * sq_a * alpha;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w);
    let a2 = (a + 1.0) + (a - 1.0) * cos_w - 2.0 * sq_a * alpha;

    BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

pub fn rbj_high_shelf(center_hz: f64, gain_db: f64, q: f64, sample_rate: f64) -> BiquadCoeffs {
    let a = libm::pow(10.0, gain_db / 40.0);
    let w0 = 2.0 * core::f64::consts::PI * center_hz / sample_rate;
    let cos_w = libm::cos(w0);
    let sin_w = libm::sin(w0);
    let alpha = sin_w / (2.0 * q);
    let sq_a = libm::sqrt(a);

    let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w + 2.0 * sq_a * alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w - 2.0 * sq_a * alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cos_w + 2.0 * sq_a * alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w);
    let a2 = (a + 1.0) - (a - 1.0) * cos_w - 2.0 * sq_a * alpha;

    BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

pub fn rbj_highpass(cutoff_hz: f64, q: f64, sample_rate: f64) -> BiquadCoeffs {
    let w0 = 2.0 * core::f64::consts::PI * cutoff_hz / sample_rate;
    let cos_w = libm::cos(w0);
    let sin_w = libm::sin(w0);
    let alpha = sin_w / (2.0 * q);

    let b0 = (1.0 + cos_w) / 2.0;
    let b1 = -(1.0 + cos_w);
    let b2 = (1.0 + cos_w) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - alpha;

    BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

pub fn rbj_lowpass(cutoff_hz: f64, q: f64, sample_rate: f64) -> BiquadCoeffs {
    let w0 = 2.0 * core::f64::consts::PI * cutoff_hz / sample_rate;
    let cos_w = libm::cos(w0);
    let sin_w = libm::sin(w0);
    let alpha = sin_w / (2.0 * q);

    let b0 = (1.0 - cos_w) / 2.0;
    let b1 = 1.0 - cos_w;
    let b2 = (1.0 - cos_w) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - alpha;

    BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

#[allow(dead_code)]
#[inline]
pub fn process_biquad(x: f32, coeffs: &[f32; 5], state: &mut [f32; 2]) -> f32 {
    let [b0, b1, b2, a1, a2] = *coeffs;
    let [w1, w2] = *state;

    let y = b0 * x + w1;
    state[0] = b1 * x - a1 * y + w2;
    state[1] = b2 * x - a2 * y;

    if libm::fabsf(state[0]) < 1e-15 {
        state[0] = 0.0;
    }
    if libm::fabsf(state[1]) < 1e-15 {
        state[1] = 0.0;
    }

    y
}

#[allow(dead_code)]
pub fn rbj_peaking_coeffs(center_hz: f32, gain_db: f32, q: f32, sample_rate: u32) -> [f32; 5] {
    let a = libm::powf(10.0_f32, gain_db / 40.0_f32);
    let w0 = 2.0 * core::f32::consts::PI * center_hz / sample_rate as f32;
    let cos_w = libm::cosf(w0);
    let sin_w = libm::sinf(w0);
    let alpha = sin_w / (2.0 * q);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos_w;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - alpha / a;

    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}
