use approx::assert_abs_diff_eq;

#[test]
fn midside_preserves_mono() {
    use sp314_dsp::limiter::MidSideProcessor;
    let mut ms = MidSideProcessor::new();
    // Mono (L=R) → Side=0 → filter does nothing
    let (l_out, r_out) = ms.process(0.5_f32, 0.5_f32);
    assert_abs_diff_eq!(l_out, 0.5_f32, epsilon = 1e-6);
    assert_abs_diff_eq!(r_out, 0.5_f32, epsilon = 1e-6);
}

#[test]
fn midside_kills_out_of_phase_dc() {
    use sp314_dsp::limiter::MidSideProcessor;
    let mut ms = MidSideProcessor::new();
    // Pure out-of-phase DC (L=1.0, R=-1.0) = pure Side signal
    // HP filter must block DC completely after settling
    let mut l_out = 0.0_f32;
    let mut r_out = 0.0_f32;
    for _ in 0..2000 {
        let (l, r) = ms.process(1.0_f32, -1.0_f32);
        l_out = l;
        r_out = r;
    }
    println!("DC after 2000 samples: L={:.2e} R={:.2e}", l_out, r_out);
    assert_abs_diff_eq!(l_out, 0.0_f32, epsilon = 1e-3);
    assert_abs_diff_eq!(r_out, 0.0_f32, epsilon = 1e-3);
}

#[test]
fn test_midside_matrix_null_roundtrip() {
    use sp314_dsp::spatial::mid_side::MidSideMatrix;

    // A diverse, realistic interleaved stereo buffer (L, R, L, R, ...)
    // Includes: exact 0, extreme values (1.0, -1.0), asymmetric panning, pure mono, phase inversion.
    let original_interleaved = vec![
        0.0_f32, 0.0_f32,       // Digital silence
        1.0, -1.0,              // Hard out-of-phase DC
        0.5, 0.5,               // Pure Mono
        0.12345, -0.67890,      // Complex asymmetric decimals
        -1.0, 1.0,              // Inverse out-of-phase DC
        0.99999, 0.00001,       // Hard pan Left
        0.00001, 0.99999,       // Hard pan Right
        -0.5, -0.5,             // Negative Mono
        -0.8765, 0.1234,        // Another complex asymmetric
        1.0, 1.0,               // Max positive Mono
    ];

    // Encode interleaved -> (Mid, Side)
    let (mid, side) = MidSideMatrix::encode(&original_interleaved);

    // Decode (Mid, Side) -> interleaved
    let reconstructed_interleaved = MidSideMatrix::decode(&mid, &side);

    assert_eq!(
        original_interleaved.len(),
        reconstructed_interleaved.len(),
        "Reconstructed buffer length must match original"
    );

    // Check each sample individually with a robust tolerance (1e-6)
    // Avoids f32::EPSILON edge cases with repeated math ops.
    for (i, (orig, recon)) in original_interleaved.iter().zip(reconstructed_interleaved.iter()).enumerate() {
        assert!(
            (orig - recon).abs() < 1e-6,
            "Sample {} mismatch: original={}, reconstructed={}, diff={}",
            i, orig, recon, (orig - recon).abs()
        );
    }
}
