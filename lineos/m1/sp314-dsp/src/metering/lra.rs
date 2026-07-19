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

/// Streaming Loudness Range (LRA) meter.
/// Memory: O(1) filter state plus hop_energies, which is O(duration) scalars
/// at one f32 per second (~115 KB for 8 hours).
pub struct StreamingLraMeter {
    block_len: usize,
    hop_len: usize,
    kf_l: KWeightingFilter,
    kf_r: KWeightingFilter,
    hop_energies: Vec<f32>,
    sum_sq: f32,
    hop_samples: usize,
    total_samples: usize,
}

impl StreamingLraMeter {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate as usize;
        let block_len = 3 * sr;
        let hop_len = sr;
        Self {
            block_len,
            hop_len,
            kf_l: KWeightingFilter::new(),
            kf_r: KWeightingFilter::new(),
            hop_energies: Vec::new(),
            sum_sq: 0.0,
            hop_samples: 0,
            total_samples: 0,
        }
    }

    pub fn process_chunk(&mut self, left: &[f32], right: &[f32]) {
        let n = left.len().min(right.len());
        self.total_samples += n;

        for i in 0..n {
            let fl = self.kf_l.process(left[i]);
            let fr = self.kf_r.process(right[i]);
            self.sum_sq += (fl * fl + fr * fr) * 0.5;
            self.hop_samples += 1;

            if self.hop_samples == self.hop_len {
                self.hop_energies.push(self.sum_sq);
                self.sum_sq = 0.0;
                self.hop_samples = 0;
            }
        }
    }

    pub fn finish(self) -> f32 {
        // Offline never filters the trailing partial hop samples at all, while
        // streaming filters them as they arrive. This cannot change the result,
        // because the filter state after the final complete hop is never read
        // again (the trailing samples are discarded here).
        let n = self.total_samples;
        if n < self.block_len {
            return 0.0;
        }

        let hop_energies = self.hop_energies;

        // Build 3s blocks by summing 3 consecutive hops
        let mut block_loudnesses: Vec<f32> = Vec::new();
        if hop_energies.len() >= 3 {
            for i in 0..=(hop_energies.len() - 3) {
                let block_sum = hop_energies[i] + hop_energies[i + 1] + hop_energies[i + 2];
                let mean_sq = block_sum / self.block_len as f32;
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

    #[test]
    fn streaming_lra_matches_offline_reference() {
        let sr: u32 = 48000;
        let block_len = 3 * sr as usize;

        let max_len = block_len * 5 + 5000;
        let mut full_signal_identical = Vec::with_capacity(max_len);
        let mut full_signal_left = Vec::with_capacity(max_len);
        let mut full_signal_right = Vec::with_capacity(max_len);

        let mut prng = 12345u32;
        for i in 0..max_len {
            let t = i as f32 / sr as f32;

            // Random noise for shared use
            prng = prng.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (prng as f32 / u32::MAX as f32) * 2.0 - 1.0;

            // VARYING loudness fixture: alternating loud/quiet plus fully silent stretch.
            // - Fully silent stretch exercises the absolute gate (> -70.0).
            // - Quiet stretch exercises the relative gate (> gamma_r).
            let sine = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * t);
            let amp = if i < block_len {
                0.8 // Loud
            } else if i < block_len * 2 {
                0.005 // Quiet (exercises relative gate)
            } else if i < block_len * 3 {
                0.8 // Loud
            } else if i < block_len * 4 {
                0.0 // Fully silent (exercises absolute gate)
            } else {
                0.5 // Medium
            };
            let sample_ident = (sine * 0.8 + noise * 0.2) * amp;
            full_signal_identical.push(sample_ident);

            // Distinct L/R fixture
            let sine_l = libm::sinf(2.0 * core::f32::consts::PI * 300.0 * t);
            let sine_r = libm::sinf(2.0 * core::f32::consts::PI * 800.0 * t);
            let amp_l = if i < block_len * 2 { 0.6 } else { 0.1 };
            let amp_r = if i > block_len && i < block_len * 3 {
                0.9
            } else {
                0.01
            };
            full_signal_left.push((sine_l * 0.9 + noise * 0.1) * amp_l);
            full_signal_right.push((sine_r * 0.7 + noise * 0.3) * amp_r);
        }

        let signal_lengths = [
            block_len - 100,             // shorter than block_len (3s)
            block_len,                   // exactly block_len
            block_len + sr as usize * 2, // a few seconds past block_len
            block_len * 2 + 1500, // non-integer number of hops (trailing partial hop discarded)
            block_len * 5,        // long-ish signal (~15s)
            0,                    // length 0
        ];

        let chunk_sizes = [4800, 4801, 1024, 48000, 1];

        // Helper to run the matrix
        let run_matrix = |left: &[f32], right: &[f32], fixture_name: &str| {
            for &len in &signal_lengths {
                let sig_l = &left[..len];
                let sig_r = &right[..len];
                let expected_lra = measure_loudness_range(sig_l, sig_r, sr);

                // Fixed chunk sizes
                for &cs in &chunk_sizes {
                    let mut meter = StreamingLraMeter::new(sr);
                    for (cl, cr) in sig_l.chunks(cs).zip(sig_r.chunks(cs)) {
                        meter.process_chunk(cl, cr);
                    }
                    let lra = meter.finish();
                    assert_eq!(
                        lra, expected_lra,
                        "LRA mismatch ({}, len={}, chunk={}). streaming_bits={:08x}, offline_bits={:08x}",
                        fixture_name, len, cs, lra.to_bits(), expected_lra.to_bits()
                    );
                }

                // Uneven mixed sequence
                let mut meter = StreamingLraMeter::new(sr);
                let mut pos = 0;
                let mut chunk_idx = 0;
                while pos < len {
                    let cs = chunk_sizes[chunk_idx % chunk_sizes.len()];
                    let end = (pos + cs).min(len);
                    meter.process_chunk(&sig_l[pos..end], &sig_r[pos..end]);
                    pos = end;
                    chunk_idx += 1;
                }
                let lra = meter.finish();
                assert_eq!(
                    lra, expected_lra,
                    "LRA mismatch ({}, len={}, chunk=mixed). streaming_bits={:08x}, offline_bits={:08x}",
                    fixture_name, len, lra.to_bits(), expected_lra.to_bits()
                );
            }
        };

        run_matrix(
            &full_signal_identical,
            &full_signal_identical,
            "Identical L/R",
        );
        run_matrix(&full_signal_left, &full_signal_right, "Distinct L/R");
    }
}
