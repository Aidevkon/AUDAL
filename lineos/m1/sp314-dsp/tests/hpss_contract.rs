#[test]
fn hpss_masks_sum_to_unity() {
    use sp314_dsp::stft::HpssProcessor;

    let n_frames = 16_usize;
    let n_bins = 64_usize;

    // Simple spectrogram: harmonic + percussive content
    let mut mag = vec![vec![0.0_f32; n_bins]; n_frames];

    // Sustained harmonic at bin 10
    for t in 0..n_frames {
        mag[t][10] = 1.0_f32;
    }

    // Percussive burst at frame 5
    for b in 0..n_bins {
        mag[5][b] = 1.0_f32;
    }

    let processor = HpssProcessor::new();
    let (mask_h, mask_p) = processor.process(&mag);

    // Masks must sum to 1.0 everywhere
    let mut max_err = 0.0_f32;
    let mut checked = 0_usize;
    for t in 0..n_frames {
        for b in 0..n_bins {
            // Only check bins with non-trivial energy
            if mag[t][b] > 0.01_f32 {
                let sum = mask_h[t][b] + mask_p[t][b];
                let err = (sum - 1.0_f32).abs();
                if err > max_err {
                    max_err = err;
                }
                checked += 1;
            }
        }
    }
    println!(
        "Checked {} active bins, max error: {:.2e}",
        checked, max_err
    );
    assert!(checked > 0, "No active bins found");
    assert!(
        max_err < 1e-5_f32,
        "Masks don't sum to 1 at active bins: {:.2e}",
        max_err
    );
}

#[test]
fn hpss_separates_harmonic_content() {
    use sp314_dsp::stft::HpssProcessor;

    // Reference value from Python fixture
    const EXPECTED_HARM: f32 = 0.95273937_f32;

    let n_frames = 64_usize;
    let n_bins = 1025_usize;

    let mut mag = vec![vec![0.0_f32; n_bins]; n_frames];

    // Sustained bins (harmonic)
    for bin in [50_usize, 100, 150, 200] {
        for t in 0..n_frames {
            mag[t][bin] = 1.0_f32;
        }
    }
    // Percussive frames
    for frame in [10_usize, 30, 50] {
        for b in 0..n_bins {
            mag[frame][b] = 1.0_f32;
        }
    }
    // Noise floor
    let mut state: u32 = 42;
    for t in 0..n_frames {
        for b in 0..n_bins {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            mag[t][b] += (state as f32 / u32::MAX as f32) * 0.05_f32;
        }
    }

    let processor = HpssProcessor::new();
    let (mask_h, _mask_p) = processor.process(&mag);

    // Harmonic mask at sustained bin 50
    let harm_mean: f32 = (0..n_frames).map(|t| mask_h[t][50]).sum::<f32>() / n_frames as f32;

    println!("Harmonic mask @ bin 50: {:.8}", harm_mean);
    println!("Expected:               {:.8}", EXPECTED_HARM);
    assert!(
        harm_mean > 0.7_f32,
        "Harmonic mask too weak: {:.4}",
        harm_mean
    );
}

#[test]
fn hpss_separates_percussive_content() {
    use sp314_dsp::stft::HpssProcessor;

    const EXPECTED_PERC: f32 = 0.97300603_f32;

    let n_frames = 64_usize;
    let n_bins = 1025_usize;

    let mut mag = vec![vec![0.0_f32; n_bins]; n_frames];

    for bin in [50_usize, 100, 150, 200] {
        for t in 0..n_frames {
            mag[t][bin] = 1.0_f32;
        }
    }
    for frame in [10_usize, 30, 50] {
        for b in 0..n_bins {
            mag[frame][b] = 1.0_f32;
        }
    }
    let mut state: u32 = 42;
    for t in 0..n_frames {
        for b in 0..n_bins {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            mag[t][b] += (state as f32 / u32::MAX as f32) * 0.05_f32;
        }
    }

    let processor = HpssProcessor::new();
    let (_mask_h, mask_p) = processor.process(&mag);

    // Percussive mask at transient frame 10
    let perc_mean: f32 = (0..n_bins).map(|b| mask_p[10][b]).sum::<f32>() / n_bins as f32;

    println!("Percussive mask @ frame 10: {:.8}", perc_mean);
    println!("Expected:                   {:.8}", EXPECTED_PERC);
    assert!(
        perc_mean > 0.7_f32,
        "Percussive mask too weak: {:.4}",
        perc_mean
    );
}
