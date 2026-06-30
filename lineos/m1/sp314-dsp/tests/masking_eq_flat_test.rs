use sp314_dsp::masking_eq::{MaskingAwareEQ, MaskingEQConfig};

#[test]
fn test_masking_eq_flat() {
    let config = MaskingEQConfig {
        target_db: [0.0; 8],
        mask_margin_db: 3.0,
        max_boost_db: 0.0,
        target_phon: 85.0,
    };

    let sample_rate = 48000;
    let mut eq = MaskingAwareEQ::new(config, sample_rate).unwrap();

    // Generate 10,000 samples of a sine wave
    let num_samples = 10000;
    let mut left = vec![0.0f32; num_samples];
    let mut right = vec![0.0f32; num_samples];

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let val = libm::sinf(2.0 * core::f32::consts::PI * 1000.0 * t);
        left[i] = val;
        right[i] = val;
    }

    let left_original = left.clone();

    // Process flat
    // stem ratios = [0.0; 5]
    let stem_ratios = [0.0; 5];

    // We need to call process_block in chunks or just one big chunk?
    // MaskingEQ process_block takes (left, right, stem_ratios)
    eq.process_block(&mut left, &mut right, &stem_ratios);

    let mut rms_in = 0.0;
    let mut rms_out = 0.0;
    for i in 0..num_samples {
        rms_in += left_original[i] * left_original[i];
        rms_out += left[i] * left[i];
    }
    rms_in = (rms_in / num_samples as f32).sqrt();
    rms_out = (rms_out / num_samples as f32).sqrt();

    println!("RMS in: {}, RMS out: {}", rms_in, rms_out);

    let delta = (rms_out - rms_in).abs() / rms_in;
    assert!(delta < 0.05, "RMS changed by {}%!", delta * 100.0);
}

/// Proves stem-aware mud correction:
/// with high harmonics+ambience ratios,
/// the 320Hz band (b=2) gets cut.
/// Compares 320Hz energy with neutral
/// stems vs muddy stems.
#[test]
fn test_masking_eq_mud_cut() {
    let sr = 48000u32;
    let n = sr as usize; // 1 second

    // 320Hz tone (mud band center)
    let make_signal = || -> (Vec<f32>, Vec<f32>) {
        let l: Vec<f32> = (0..n)
            .map(|i| (2.0 * core::f32::consts::PI * 320.0 * i as f32 / sr as f32).sin() * 0.5)
            .collect();
        let r = l.clone();
        (l, r)
    };

    let cfg = MaskingEQConfig {
        target_db: [0.0; 8],
        mask_margin_db: 3.0,
        max_boost_db: 0.0,
        target_phon: 80.0,
    };

    // Run A: neutral stems [0.0;5]
    let (mut la, mut ra) = make_signal();
    let mut eq_a = MaskingAwareEQ::new(cfg.clone(), sr).unwrap();
    let neutral = [0.0_f32; 5];
    // process in blocks
    let block = 512;
    let mut pos = 0;
    while pos < n {
        let end = (pos + block).min(n);
        eq_a.process_block(&mut la[pos..end], &mut ra[pos..end], &neutral);
        pos = end;
    }
    let rms_neutral = (la.iter().map(|s| s * s).sum::<f32>() / la.len() as f32).sqrt();

    // Run B: muddy stems
    // [bass, harmonics, voice, drums, ambience]
    // harmonics(1) = 0.80 > 0.40 → penalty
    // = (0.80-0.40)*5 = 2.0dB cut at 320Hz.
    // (Ambience excluded from mud formula:
    // it is the NMF catch-all bucket.)
    let (mut lb, mut rb) = make_signal();
    let mut eq_b = MaskingAwareEQ::new(cfg.clone(), sr).unwrap();
    let muddy = [0.05, 0.80, 0.05, 0.05, 0.05];
    let mut pos = 0;
    while pos < n {
        let end = (pos + block).min(n);
        eq_b.process_block(&mut lb[pos..end], &mut rb[pos..end], &muddy);
        pos = end;
    }
    let rms_muddy = (lb.iter().map(|s| s * s).sum::<f32>() / lb.len() as f32).sqrt();

    println!(
        "Mud cut test: rms_neutral={:.4} \
         rms_muddy={:.4} \
         (muddy should be lower — \
         320Hz cut)",
        rms_neutral, rms_muddy
    );

    // muddy stems → 320Hz cut → lower RMS
    assert!(
        rms_muddy < rms_neutral * 0.95,
        "Mud correction failed: muddy \
         RMS {:.4} not lower than neutral \
         {:.4}",
        rms_muddy,
        rms_neutral
    );
}
