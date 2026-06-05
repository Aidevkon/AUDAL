use sp314_dsp::stft::stem_renderer::StemRenderer;

#[test]
fn stem_render_percussive_captures_impulse() {
    let fft_size    = 2048_usize;
    let hop_size    = 512_usize;
    let fs          = 48000_f32;
    let n_samples   = fft_size * 8;
    let impulse_pos = 4 * hop_size + fft_size / 2;  // 3072

    let mut signal = vec![0.0_f32; n_samples];
    for i in 0..n_samples {
        let t = i as f32 / fs;
        signal[i] = 0.5_f32 * libm::sinf(2.0_f32 * core::f32::consts::PI * 440.0_f32 * t);
    }
    signal[impulse_pos] += 0.8_f32;

    let mut renderer = StemRenderer::new();
    let (stem_h, stem_p) = renderer.render(&signal);

    let p_at_impulse = stem_p[impulse_pos];
    let h_at_impulse = stem_h[impulse_pos];

    println!("Percussive at impulse: {:.4}", p_at_impulse);
    println!("Harmonic at impulse:   {:.4}", h_at_impulse);

    assert!(p_at_impulse.abs() > h_at_impulse.abs(),
        "Percussive {:.4} should exceed harmonic {:.4} at impulse", p_at_impulse, h_at_impulse);
    assert!(p_at_impulse.abs() > 0.3_f32,
        "Percussive energy too low at impulse: {:.4}", p_at_impulse);
}

#[test]
fn stem_render_harmonic_captures_sine() {
    let fft_size  = 2048_usize;
    let fs        = 48000_f32;
    let n_samples = fft_size * 8;

    let mut signal = vec![0.0_f32; n_samples];
    for i in 0..n_samples {
        let t = i as f32 / fs;
        signal[i] = 0.5_f32 * libm::sinf(2.0_f32 * core::f32::consts::PI * 440.0_f32 * t);
    }

    let mut renderer = StemRenderer::new();
    let (stem_h, stem_p) = renderer.render(&signal);

    let mid = n_samples / 2;
    let harm_energy: f32 = stem_h[mid..mid+1000].iter().map(|x| x.abs()).sum::<f32>() / 1000.0;
    let drum_energy: f32 = stem_p[mid..mid+1000].iter().map(|x| x.abs()).sum::<f32>() / 1000.0;

    println!("Harmonic energy at sine region: {:.4}", harm_energy);
    println!("Drums energy at sine region:    {:.4}", drum_energy);

    assert!(harm_energy > drum_energy,
        "Harmonic {:.4} should exceed drums {:.4}", harm_energy, drum_energy);
    assert!(harm_energy > 0.1_f32,
        "Harmonic energy too low: {:.4}", harm_energy);
}

#[test]
fn stem_render_lengths_match_input() {
    let n = 2048_usize * 4;
    let signal = vec![0.1_f32; n];
    let mut renderer = StemRenderer::new();
    let (stem_h, stem_p) = renderer.render(&signal);

    assert_eq!(stem_h.len(), n, "Harmonic length mismatch");
    assert_eq!(stem_p.len(), n, "Drums length mismatch");
}

#[test]
fn stem_render_reconstruction_within_mastering_tolerance() {
    let fft_size  = 2048_usize;
    let fs        = 48000_f32;
    let n_samples = fft_size * 8;

    // Complex mix
    let mut signal = vec![0.0_f32; n_samples];
    for i in 0..n_samples {
        let t = i as f32 / fs;
        signal[i] = 0.5_f32 * libm::sinf(2.0_f32 * core::f32::consts::PI * 440.0_f32 * t);
    }
    signal[3072] += 0.8_f32;

    let mut renderer = StemRenderer::new();
    let (stem_h, stem_p) = renderer.render(&signal);

    let margin = fft_size; // Skip edges where OLA isn't fully built
    let mut max_err = 0.0_f32;
    for i in margin..n_samples - margin {
        let sum = stem_h[i] + stem_p[i];
        let err = (signal[i] - sum).abs();
        if err > max_err { max_err = err; }
    }

    println!("Reconstruction Max Error: {:.2e}", max_err);
    // Was: perfect reconstruction (< 1e-5)
    // Now: reconstruction within mastering tolerance (< 5e-2)
    // Mask Refinement (spectral gating + FIR) intentionally modifies masks
    assert!(max_err < 5e-2_f32, "Reconstruction error too high: {:.2e}", max_err);
}
