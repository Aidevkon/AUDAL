pub fn compute_gain_reduction(
    envelope_db:  f32,
    threshold_db: f32,
    ratio:        f32,
    knee_db:      f32,
) -> f32 {
    let overshoot = envelope_db - threshold_db;
    let slope_diff = 1.0 - (1.0 / ratio);

    if overshoot <= -knee_db / 2.0 {
        // Below knee: no compression
        0.0
    } else if libm::fabsf(overshoot) < knee_db / 2.0 {
        // Soft knee region: parabolic interpolation
        // C1 continuous — no "corner" that causes harmonic distortion
        let x = overshoot + knee_db / 2.0;
        -(slope_diff * x * x) / (2.0 * knee_db)
    } else {
        // Above knee: full compression
        -slope_diff * overshoot
    }
}
