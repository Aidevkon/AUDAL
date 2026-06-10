#[test]
fn stft_perfect_reconstruction_sine() {
    use sp314_dsp::stft::{StftEngine, FFT_SIZE};

    let mut engine = StftEngine::new();
    let n = FFT_SIZE * 8; // 8 frames
    let fs = 48000.0_f32;

    // 1kHz sine at 0.5 amplitude
    let signal: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / fs;
            0.5_f32 * (2.0_f32 * std::f32::consts::PI * 1000.0_f32 * t).sin()
        })
        .collect();

    // Round-trip: STFT → iSTFT
    let (frames, _) = engine.forward(&signal);
    let reconstructed = engine.inverse(&frames, n);

    // Skip edges (first and last FFT_SIZE samples)
    let margin = FFT_SIZE;
    let mut max_err = 0.0_f32;
    for i in margin..n - margin {
        let err = (signal[i] - reconstructed[i]).abs();
        if err > max_err {
            max_err = err;
        }
    }

    println!("Sine reconstruction max error: {:.2e}", max_err);
    assert!(
        max_err < 1e-5_f32,
        "Perfect reconstruction failed: {:.2e}",
        max_err
    );
}

#[test]
fn stft_perfect_reconstruction_noise() {
    use sp314_dsp::stft::{StftEngine, FFT_SIZE};

    let mut engine = StftEngine::new();
    let n = FFT_SIZE * 8;

    // Deterministic white noise (fixed pattern)
    // Use simple LFSR-like pattern — no rand crate needed
    let mut signal = vec![0.0_f32; n];
    let mut state: u32 = 12345;
    for s in signal.iter_mut() {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *s = (state as f32 / u32::MAX as f32) * 2.0_f32 - 1.0_f32;
        *s *= 0.3_f32;
    }

    let (frames, _) = engine.forward(&signal);
    let reconstructed = engine.inverse(&frames, n);

    let margin = FFT_SIZE;
    let mut max_err = 0.0_f32;
    for i in margin..n - margin {
        let err = (signal[i] - reconstructed[i]).abs();
        if err > max_err {
            max_err = err;
        }
    }

    println!("Noise reconstruction max error: {:.2e}", max_err);
    assert!(
        max_err < 1e-5_f32,
        "Perfect reconstruction failed: {:.2e}",
        max_err
    );
}

#[test]
fn stft_output_dimensions() {
    use sp314_dsp::stft::{StftEngine, FFT_SIZE, HOP_SIZE, N_BINS};

    let mut engine = StftEngine::new();
    let n = FFT_SIZE * 4;
    let signal = vec![0.5_f32; n];

    let (frames, n_frames) = engine.forward(&signal);

    // Expected frames: with FFT_SIZE/2 padding on both sides, length is n + FFT_SIZE
    let padded_n = n + FFT_SIZE;
    let expected_frames = (padded_n - FFT_SIZE) / HOP_SIZE + 1;
    println!("n_frames: {}, expected: {}", n_frames, expected_frames);

    assert_eq!(n_frames, expected_frames, "Wrong frame count");
    assert_eq!(frames.len(), n_frames, "frames.len() mismatch");
    assert_eq!(
        frames[0].len(),
        N_BINS,
        "Wrong bin count: {}",
        frames[0].len()
    );
}
