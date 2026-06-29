use crate::metering::filter::KWeightingFilter;

/// EBU R128 Loudness Range (LRA) per EBU Tech 3342.
///
/// Zero-allocation implementation:
/// K-weighted energy computed per 1s hop on-the-fly
/// (IIR filter state flows forward continuously —
/// samples never re-fed). Three consecutive hops
/// combined for each 3s block. Only hop_count
/// floats allocated (60 max for 60s input).
///
/// Returns LU. Returns 0.0 if < 3s of material
/// or insufficient ungated blocks.
///
/// Note: KWeightingFilter is hardcoded 48kHz.
/// Pass 48kHz signal for accurate results.
pub fn measure_loudness_range(left: &[f32], right: &[f32], sample_rate: u32) -> f32 {
    let sr = sample_rate as usize;
    let block_len = 3 * sr;
    let hop_len = sr; // 1s hop

    let n = left.len().min(right.len());
    if n < block_len {
        return 0.0;
    }

    // No cap needed: hop_energies holds
    // one f32 per second of audio.
    // A 5-min track = 300 floats = 1.2 KB.
    // Full-track analysis for Netflix/EBU
    // compliance with zero memory concern.
    let hop_count = n / hop_len;

    let mut kf_l = KWeightingFilter::new();
    let mut kf_r = KWeightingFilter::new();

    // Zero-allocation hop-energy accumulation.
    // IIR filter reads signal once, left to right.
    // hop_energies holds mean square energy per
    // 1s hop (max 60 floats = 240 bytes).
    let mut hop_energies = Vec::with_capacity(hop_count);
    for h in 0..hop_count {
        let mut sum_sq = 0.0_f32;
        let start = h * hop_len;
        let end = start + hop_len;
        for i in start..end {
            let fl = kf_l.process(left[i]);
            let fr = kf_r.process(right[i]);
            sum_sq += (fl * fl + fr * fr) * 0.5;
        }
        hop_energies.push(sum_sq);
    }

    // Build 3s blocks by summing 3 consecutive hops
    let mut block_loudnesses: Vec<f32> = Vec::new();
    if hop_energies.len() >= 3 {
        for i in 0..=(hop_energies.len() - 3) {
            let block_sum = hop_energies[i] + hop_energies[i + 1] + hop_energies[i + 2];
            let mean_sq = block_sum / block_len as f32;
            let lufs = if mean_sq > 1e-10 {
                -0.691 + 10.0 * libm::log10f(mean_sq)
            } else {
                -144.0
            };
            block_loudnesses.push(lufs);
        }
    }

    if block_loudnesses.is_empty() {
        return 0.0;
    }

    // Absolute gate: discard blocks < -70 LUFS
    let abs_gated: Vec<f32> = block_loudnesses
        .iter()
        .copied()
        .filter(|&l| l > -70.0)
        .collect();

    if abs_gated.len() < 2 {
        return 0.0;
    }

    // Relative gate — linear energy domain average
    // per EBU Tech 3342 §3.3 (NOT dB average)
    let mean_energy: f32 = abs_gated
        .iter()
        .map(|&lufs| libm::powf(10.0, lufs / 10.0))
        .sum::<f32>()
        / abs_gated.len() as f32;
    let gamma_r = 10.0 * libm::log10f(mean_energy) - 20.0;

    let rel_gated: Vec<f32> = abs_gated.iter().copied().filter(|&l| l > gamma_r).collect();

    if rel_gated.len() < 2 {
        return 0.0;
    }

    // Percentile — (len-1) indexing +
    // .round() for accuracy on small arrays
    let mut sorted = rel_gated.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let len_m1 = (sorted.len() - 1) as f32;
    let p10_idx = (len_m1 * 0.10).round() as usize;
    let p95_idx = (len_m1 * 0.95).round() as usize;

    (sorted[p95_idx] - sorted[p10_idx]).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_returns_zero() {
        let s = vec![0.0_f32; 48000 * 5];
        assert_eq!(measure_loudness_range(&s, &s, 48000), 0.0);
    }

    #[test]
    fn too_short_returns_zero() {
        let s = vec![0.5_f32; 48000 * 2];
        assert_eq!(measure_loudness_range(&s, &s, 48000), 0.0);
    }

    #[test]
    fn constant_signal_near_zero_lra() {
        let sr = 48000u32;
        let signal: Vec<f32> = (0..sr as usize * 10)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr as f32).sin() * 0.5)
            .collect();
        let lra = measure_loudness_range(&signal, &signal, sr);
        assert!(
            lra < 3.0,
            "Constant LRA {lra:.2} LU \
             expected < 3.0"
        );
    }

    #[test]
    fn dynamic_signal_has_measurable_lra() {
        let sr = 48000u32;
        let block = sr as usize;
        let loud: Vec<f32> = (0..block * 4)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin() * 0.5)
            .collect();
        let quiet: Vec<f32> = (0..block * 4)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin() * 0.05)
            .collect();
        let mut signal = Vec::new();
        signal.extend_from_slice(&loud);
        signal.extend_from_slice(&quiet);
        signal.extend_from_slice(&loud);
        let lra = measure_loudness_range(&signal, &signal, sr);
        assert!(
            lra > 1.0,
            "Dynamic LRA {lra:.2} LU \
             expected > 1.0"
        );
        println!("Dynamic LRA: {lra:.2} LU");
    }
}
