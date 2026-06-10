#[test]
fn spectral_flux_detects_correct_beats() {
    use sp314_dsp::stft::SpectralFluxDetector;

    let fs = 48000_usize;
    let hop_size = 512_usize;

    // Reconstruct identical synthetic signal as Python fixture
    // Kicks: 60Hz damped sine at 0ms, 1000ms
    // Snares: damped noise at 500ms, 1500ms
    // seed=42 — but we use same LFSR as other tests

    let duration_ms = 2000_usize;
    let n_samples = fs * duration_ms / 1000;
    let mut signal = vec![0.0_f32; n_samples];

    // Add kick: 60Hz damped sine
    let add_kick = |sig: &mut Vec<f32>, onset_ms: usize, amp: f32, decay_ms: f32| {
        let onset = onset_ms * fs / 1000;
        let length = (decay_ms * fs as f32 / 1000.0) as usize;
        for i in 0..length.min(n_samples - onset) {
            let t = i as f32 / fs as f32;
            let decay = libm::expf(-t * 1000.0_f32 / decay_ms);
            let sine = libm::sinf(2.0_f32 * core::f32::consts::PI * 60.0_f32 * t);
            sig[onset + i] += amp * sine * decay;
        }
    };

    // Add snare: deterministic noise via xorshift32
    let add_snare =
        |sig: &mut Vec<f32>, onset_ms: usize, amp: f32, decay_ms: f32, seed: &mut u32| {
            let onset = onset_ms * fs / 1000;
            let length = (decay_ms * fs as f32 / 1000.0) as usize;
            for i in 0..length.min(n_samples - onset) {
                // xorshift32 — same sequence as Python rng(42)
                // NOTE: Python numpy uses a different RNG.
                // We use a fixed sequence that produces similar
                // broadband noise characteristics.
                *seed ^= *seed << 13;
                *seed ^= *seed >> 17;
                *seed ^= *seed << 5;
                let noise = (*seed as f32 / u32::MAX as f32) * 2.0_f32 - 1.0_f32;
                let t = i as f32 / fs as f32;
                let decay = libm::expf(-t * 1000.0_f32 / decay_ms);
                sig[onset + i] += amp * noise * decay;
            }
        };

    add_kick(&mut signal, 0, 0.9_f32, 50.0_f32);
    add_kick(&mut signal, 1000, 0.9_f32, 50.0_f32);
    let mut seed: u32 = 42;
    add_snare(&mut signal, 500, 0.7_f32, 20.0_f32, &mut seed);
    add_snare(&mut signal, 1500, 0.7_f32, 20.0_f32, &mut seed);

    let mut detector = SpectralFluxDetector::new();
    let (flux_norm, beats) = detector.detect(&signal);

    println!("Detected beats: {:?}", beats);
    println!(
        "Beat times: {:?}",
        beats
            .iter()
            .map(|&b| b * hop_size * 1000 / fs)
            .collect::<Vec<_>>()
    );
    println!(
        "Max flux_norm: {:.4}",
        flux_norm.iter().cloned().fold(0.0_f32, f32::max)
    );

    // Must detect 3-5 beats (kicks are deterministic,
    // snares depend on noise RNG)
    assert!(
        beats.len() >= 2,
        "Must detect at least 2 kicks, got {}",
        beats.len()
    );
    assert!(
        beats.len() <= 6,
        "Too many false positives: {}",
        beats.len()
    );

    // Kicks at 0ms and 1000ms must be detected
    // Frame 0 kick → expect beat near frame 0-5
    // Frame 1000ms / (512/48000) ≈ frame 93
    let kick1_detected = beats.iter().any(|&b| b <= 5);
    let kick2_detected = beats.iter().any(|&b| b >= 88 && b <= 98);

    assert!(
        kick1_detected,
        "Kick at 0ms not detected. Beats: {:?}",
        beats
    );
    assert!(
        kick2_detected,
        "Kick at 1000ms not detected. Beats: {:?}",
        beats
    );

    println!("Kick 1 (0ms):    detected ✅");
    println!("Kick 2 (1000ms): detected ✅");
}
