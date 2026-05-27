// analysis/stereo.rs — Stereo feature extraction
// libm only. Input: stereo interleaved [L,R,L,R,...]

/// Stereo correlation: Σ(L×R) / sqrt(Σ(L²)×Σ(R²))
/// Range: [-1.0, 1.0]
pub fn stereo_correlation(stereo: &[f32]) -> f32 {
    if stereo.len() < 2 { return 1.0; }

    let l: Vec<f32> = stereo.iter().step_by(2).copied().collect();
    let r: Vec<f32> = stereo.iter().skip(1).step_by(2).copied().collect();

    let n = l.len().min(r.len());
    if n == 0 { return 1.0; }

    let mut cross = 0.0_f32;
    let mut sum_l = 0.0_f32;
    let mut sum_r = 0.0_f32;

    for i in 0..n {
        cross += l[i] * r[i];
        sum_l += l[i] * l[i];
        sum_r += r[i] * r[i];
    }

    let denom = libm::sqrtf(sum_l * sum_r);
    if denom < 1e-10 { return 1.0; }

    (cross / denom).clamp(-1.0, 1.0)
}

/// Stereo width: 1.0 - |correlation|
/// Range: [0.0, 1.0]
pub fn stereo_width(stereo: &[f32]) -> f32 {
    let corr = stereo_correlation(stereo);
    1.0 - libm::fabsf(corr)
}
