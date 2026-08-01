use sp314_dsp::analysis::mel_128::{
    expand_mask_to_linear, fold_to_mel, MEL_128_INVERSE, MEL_128_MATRIX, MEL_BANDS, N_BINS,
};

fn generate_noise(samples: usize, seed_init: u32) -> Vec<f32> {
    let mut buf = vec![0.0; samples];
    let mut seed = seed_init;
    for s in buf.iter_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let val = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
        *s = val;
    }
    buf
}

#[test]
fn test_mel_128_fold_determinism_shape() {
    let frame = generate_noise(N_BINS, 42);
    let mut arr = [0.0; N_BINS];
    arr.copy_from_slice(&frame);

    let mel1 = fold_to_mel(&arr);
    let mel2 = fold_to_mel(&arr);

    for i in 0..MEL_BANDS {
        assert_eq!(mel1[i].to_bits(), mel2[i].to_bits());
    }
    assert_eq!(mel1.len(), MEL_BANDS);
}

#[test]
fn test_mel_128_round_trip_landscape() {
    // White noise frame folded, then an all-ones mask expanded
    let mask_ones = [1.0f32; MEL_BANDS];

    let linear_mask = expand_mask_to_linear(&mask_ones);

    let mut max_dev = 0.0f32;
    for &val in linear_mask.iter() {
        let dev = (val - 1.0).abs();
        if dev > max_dev {
            max_dev = dev;
        }
    }

    assert!(
        max_dev < 1e-6,
        "Membership weights must normalize to 1.0. Max deviation: {}",
        max_dev
    );
}

#[test]
fn test_mel_128_shaped_mask_transition() {
    let mut shaped_mask = [0.0f32; MEL_BANDS];
    // Low bands 1.0, high bands 0.0
    let split = MEL_BANDS / 2; // band 64
    for i in 0..split {
        shaped_mask[i] = 1.0;
    }

    let linear_mask = expand_mask_to_linear(&shaped_mask);

    // Find the transition profile: bins strictly between 0.0 and 1.0
    // Actually because it's a weighted sum of bands, and the bands are triangular,
    // there's a smearing effect.
    let mut transition_start = None;
    let mut transition_end = None;

    for (b, &val) in linear_mask.iter().enumerate() {
        if val < 0.999 && transition_start.is_none() {
            transition_start = Some(b);
        }
        if val < 0.001 && transition_start.is_some() && transition_end.is_none() {
            transition_end = Some(b);
        }
    }

    let start = transition_start.unwrap_or(0);
    let end = transition_end.unwrap_or(N_BINS - 1);
    let width = end - start;

    println!(
        "MEL128|SHAPED_MASK|transition_start_bin={}|transition_end_bin={}|smearing_width_bins={}",
        start, end, width
    );
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0f32.powf(mel / 2595.0) - 1.0)
}
fn create_mel_filterbank(sr: f32, n_fft: usize, n_mels: usize) -> Vec<Vec<f32>> {
    let n_bins = n_fft / 2 + 1;
    let min_mel = hz_to_mel(0.0);
    let max_mel = hz_to_mel(sr / 2.0);
    let mel_points: Vec<f32> = (0..(n_mels + 2))
        .map(|i| min_mel + i as f32 * (max_mel - min_mel) / (n_mels + 1) as f32)
        .collect();
    let hz_points: Vec<f32> = mel_points.into_iter().map(mel_to_hz).collect();

    let bin_freqs: Vec<f32> = (0..n_bins).map(|i| i as f32 * sr / n_fft as f32).collect();

    let mut fbank = vec![vec![0.0f32; n_bins]; n_mels];
    for i in 0..n_mels {
        let f_m_minus = hz_points[i];
        let f_m = hz_points[i + 1];
        let f_m_plus = hz_points[i + 2];
        for b in 0..n_bins {
            let freq = bin_freqs[b];
            if freq >= f_m_minus && freq <= f_m {
                fbank[i][b] = (freq - f_m_minus) / (f_m - f_m_minus);
            } else if freq >= f_m && freq <= f_m_plus {
                fbank[i][b] = (f_m_plus - freq) / (f_m_plus - f_m);
            }
        }
    }
    fbank
}

#[test]
fn diagnose_mismatch() {
    let old = create_mel_filterbank(48000.0, 2048, 128);
    let mut diff_count = 0;
    let mut max_abs = 0.0f32;
    let mut examples = Vec::new();

    for m in 0..128 {
        for b in 0..1025 {
            let old_val = old[m][b];
            let new_val = MEL_128_MATRIX[m][b];
            if old_val.to_bits() != new_val.to_bits() {
                diff_count += 1;
                let diff = (old_val - new_val).abs();
                if diff > max_abs {
                    max_abs = diff;
                }
                if examples.len() < 3 {
                    examples.push(format!(
                        "(m={}, b={}, old=0x{:08x}, new=0x{:08x})",
                        m,
                        b,
                        old_val.to_bits(),
                        new_val.to_bits()
                    ));
                }
            }
        }
    }

    println!("DIAGNOSE|diff_count={}|max_abs={:e}", diff_count, max_abs);
    for ex in examples {
        println!("DIAGNOSE_EXAMPLE|{}", ex);
    }
}
