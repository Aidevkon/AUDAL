// analysis/dynamics.rs — Dynamics feature extraction
// libm only.

/// Crest factor in dB: peak / RMS.
/// High = percussive (drums). Low = compressed.
pub fn crest_factor_db(signal: &[f32]) -> f32 {
    if signal.is_empty() { return 10.0; }

    let mut peak = 0.0_f32;
    let mut sum_sq = 0.0_f32;

    for &s in signal {
        let abs_s = libm::fabsf(s);
        if abs_s > peak { peak = abs_s; }
        sum_sq += s * s;
    }

    let rms = libm::sqrtf(sum_sq / signal.len() as f32);
    if rms < 1e-10 { return 0.0; }

    20.0 * libm::log10f(peak / rms)
}

/// RMS in dBFS.
pub fn rms_db(signal: &[f32]) -> f32 {
    if signal.is_empty() { return -144.0; }

    let sum_sq: f32 = signal.iter().map(|s| s * s).sum();
    let mean_sq = sum_sq / signal.len() as f32;

    if mean_sq < 1e-30 { return -144.0; }

    10.0 * libm::log10f(mean_sq)
}

/// Dynamic range in dBFS:
/// 95th percentile - 5th percentile of block RMS values.
/// Block size: DYNAMIC_RANGE_BLOCK_MS at given sample_rate.
pub fn dynamic_range_db(signal: &[f32], sample_rate: u32) -> f32 {
    use super::features::DYNAMIC_RANGE_BLOCK_MS;

    let block_size = (sample_rate as usize * DYNAMIC_RANGE_BLOCK_MS as usize)
        / 1000;
    if block_size == 0 || signal.len() < block_size { return 0.0; }

    let mut block_rms: Vec<f32> = signal
        .chunks(block_size)
        .filter(|c| c.len() == block_size)
        .map(|c| rms_db(c))
        .filter(|&r| r > -144.0)
        .collect();

    if block_rms.is_empty() { return 0.0; }
    block_rms.sort_by(|a, b| a.total_cmp(b));

    let n    = block_rms.len();
    let p95  = block_rms[(n * 95 / 100).min(n - 1)];
    let p5   = block_rms[(n * 5  / 100).min(n - 1)];
    p95 - p5
}
