// analysis/stereo.rs — Stereo feature extraction
// libm only. Input: stereo interleaved [L,R,L,R,...]

/// Stereo correlation: Σ(L×R) / sqrt(Σ(L²)×Σ(R²))
/// Range: [-1.0, 1.0]
pub fn stereo_correlation(left: &[f32], right: &[f32]) -> f32 {
    let n = left.len().min(right.len());
    if n == 0 {
        return 1.0;
    }

    let mut cross = 0.0_f32;
    let mut sum_l = 0.0_f32;
    let mut sum_r = 0.0_f32;

    for i in 0..n {
        cross += left[i] * right[i];
        sum_l += left[i] * left[i];
        sum_r += right[i] * right[i];
    }

    let denom = libm::sqrtf(sum_l * sum_r);
    if denom < 1e-10 {
        return 1.0;
    }

    (cross / denom).clamp(-1.0, 1.0)
}

/// Stereo width: 1.0 - |correlation|
/// Range: [0.0, 1.0]
pub fn stereo_width(left: &[f32], right: &[f32]) -> f32 {
    let corr = stereo_correlation(left, right);
    1.0 - libm::fabsf(corr)
}
