use approx::assert_abs_diff_eq;

#[test]
fn harmonic_no_aliasing_at_high_freq() {
    use sp314_dsp::harmonic::{HarmonicConfig, HarmonicEngine};

    let config = HarmonicConfig {
        drive: 2.0,
        drive_compensation: 1.995_261_7,
        even_amount: 0.6,
        odd_amount: 0.2,
        mix: 0.3,
    };
    let mut engine = HarmonicEngine::new(config);

    let fs = 48000_f32;
    let n_samples = 4800_usize; // 100ms
    let freq = 10000_f32; // 10kHz — 3rd harmonic = 30kHz

    // Generate 10kHz sine
    let signal: Vec<f32> = (0..n_samples)
        .map(|i| {
            let t = i as f32 / fs;
            0.5_f32 * libm::sinf(2.0_f32 * core::f32::consts::PI * freq * t)
        })
        .collect();

    // Process through HarmonicEngine
    let mut output = vec![0.0_f32; n_samples];
    for i in 0..n_samples {
        let (m, _) = engine.process_frame(signal[i], 0.0);
        output[i] = m;
    }

    // After settling, output must be bounded
    let margin = 100_usize;
    let max_out = output[margin..]
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max);

    println!("Max output amplitude: {:.4}", max_out);

    // Must not exceed 1.0 (no explosive aliasing)
    assert!(max_out < 1.0_f32, "Output exceeded bounds: {:.4}", max_out);

    // Must have signal (not silence)
    assert!(
        max_out > 0.01_f32,
        "Output suspiciously silent: {:.4}",
        max_out
    );
}

#[test]
fn harmonic_bypass_when_mix_zero() {
    use sp314_dsp::harmonic::{HarmonicConfig, HarmonicEngine};

    let config = HarmonicConfig {
        mix: 0.0,
        ..HarmonicConfig::default()
    };
    let mut engine = HarmonicEngine::new(config);

    let (m_out, s_out) = engine.process_frame(0.5, -0.3);
    assert_eq!(m_out, 0.5_f32, "Bypass changed mid");
    assert_eq!(s_out, -0.3_f32, "Bypass changed side");
}

#[test]
fn harmonic_unity_gain_on_silence() {
    use sp314_dsp::harmonic::{HarmonicConfig, HarmonicEngine};

    let mut engine = HarmonicEngine::new(HarmonicConfig::default());
    let (m, s) = engine.process_frame(0.0, 0.0);
    assert_abs_diff_eq!(m, 0.0_f32, epsilon = 1e-6_f32);
    assert_abs_diff_eq!(s, 0.0_f32, epsilon = 1e-6_f32);
}
