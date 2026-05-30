#[test]
fn four_stem_lengths_match_input() {
    use sp314_dsp::stft::stem_renderer::FourStemRenderer;

    let n = 2048 * 8;
    let signal = vec![0.5_f32; n];
    let mut renderer = FourStemRenderer::new();
    let stems = renderer.render(&signal);

    assert_eq!(stems.bass.len(),   n);
    assert_eq!(stems.harmonics.len(), n);
    assert_eq!(stems.drums.len(),  n);
    assert_eq!(stems.ambience.len(),  n);
}

#[test]
fn four_stem_perfect_reconstruction() {
    use sp314_dsp::stft::stem_renderer::FourStemRenderer;

    let fft_size = 2048_usize;
    let fs       = 48000_f32;
    let n        = fft_size * 8;

    // 440Hz sine
    let signal: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / fs;
            0.5_f32 * libm::sinf(
                2.0_f32 * core::f32::consts::PI * 440.0_f32 * t)
        })
        .collect();

    let mut renderer = FourStemRenderer::new();
    let stems = renderer.render(&signal);

    // Sum of all 4 stems must equal original
    let margin = fft_size;
    let mut max_err = 0.0_f32;
    for i in margin..n - margin {
        let sum = stems.bass[i]
                + stems.harmonics[i]
                + stems.drums[i]
                + stems.ambience[i];
        let err = (signal[i] - sum).abs();
        if err > max_err { max_err = err; }
    }

    println!("4-stem reconstruction error: {:.2e}", max_err);
    assert!(max_err < 1e-4_f32,
        "Perfect reconstruction failed: {:.2e}", max_err);
}

#[test]
fn four_stem_drums_captures_transient() {
    use sp314_dsp::stft::stem_renderer::FourStemRenderer;

    let fft_size    = 2048_usize;
    let fs          = 48000_f32;
    let n           = fft_size * 8;
    let impulse_pos = 4 * 512 + fft_size / 2;

    // Sine + impulse
    let mut signal: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / fs;
            0.3_f32 * libm::sinf(
                2.0_f32 * core::f32::consts::PI * 440.0_f32 * t)
        })
        .collect();
    signal[impulse_pos] += 0.8_f32;

    let mut renderer = FourStemRenderer::new();
    let stems = renderer.render(&signal);

    // Drums must have more energy at impulse than bass
    let drums_energy = stems.drums[impulse_pos].abs();
    let bass_energy  = stems.bass[impulse_pos].abs();

    println!("Drums at impulse: {:.4}", drums_energy);
    println!("Bass at impulse:  {:.4}", bass_energy);

    assert!(drums_energy > bass_energy,
        "Drums {:.4} should exceed bass {:.4} at impulse",
        drums_energy, bass_energy);
}
