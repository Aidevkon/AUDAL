#[inline]
pub fn process_biquad(x: f32, coeffs: &[f32; 5], state: &mut [f32; 2]) -> f32 {
    let [b0, b1, b2, a1, a2] = *coeffs;
    let [w1, w2] = *state;

    let y = b0 * x + w1;
    state[0] = b1 * x - a1 * y + w2;
    state[1] = b2 * x - a2 * y;

    if libm::fabsf(state[0]) < 1e-15 { state[0] = 0.0; }
    if libm::fabsf(state[1]) < 1e-15 { state[1] = 0.0; }

    y
}

pub fn rbj_peaking_coeffs(center_hz: f32, gain_db: f32, q: f32, sample_rate: u32) -> [f32; 5] {
    let a     = libm::powf(10.0_f32, gain_db / 40.0_f32);
    let w0    = 2.0 * core::f32::consts::PI * center_hz / sample_rate as f32;
    let cos_w = libm::cosf(w0);
    let sin_w = libm::sinf(w0);
    let alpha = sin_w / (2.0 * q);

    let b0 =  1.0 + alpha * a;
    let b1 = -2.0 * cos_w;
    let b2 =  1.0 - alpha * a;
    let a0 =  1.0 + alpha / a;
    let a1 = -2.0 * cos_w;
    let a2 =  1.0 - alpha / a;

    [b0/a0, b1/a0, b2/a0, a1/a0, a2/a0]
}
