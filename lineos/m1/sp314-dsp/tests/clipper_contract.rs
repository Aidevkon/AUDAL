#[test]
fn soft_clipper_transparent_on_low_signals() {
    use sp314_dsp::limiter::OversampledSoftClipper;
    let mut clipper = OversampledSoftClipper::new(true);
    let mut max_out = 0.0_f32;

    // -20dB (0.1 amplitude) sine — clipper must preserve amplitude.
    // We measure peak amplitude of OUTPUT only — NOT sample-by-sample diff.
    // Reason: FIR group delay (~16 samples) makes direct subtraction
    // meaningless (phase shift ≠ amplitude change).
    for i in 0..2000 {
        let t = i as f32 / 48000.0_f32;
        let sig = (t * 2.0_f32 * std::f32::consts::PI
                   * 1000.0_f32).sin() * 0.1_f32;
        let (l_out, _) = clipper.process(sig, sig);
        // Skip settling period (FIR delay ~100 samples)
        if i > 100 {
            if l_out.abs() > max_out { max_out = l_out.abs(); }
        }
    }
    println!("Transparent max output: {:.6}", max_out);
    // Peak must be very close to 0.1 (tiny FIR ripple allowed ±0.005)
    let diff = (max_out - 0.1_f32).abs();
    assert!(diff < 0.005_f32,
        "Clipper altered low signal amplitude! Max out: {:.6}", max_out);
}

#[test]
fn soft_clipper_limits_massive_peaks() {
    use sp314_dsp::limiter::OversampledSoftClipper;
    let mut clipper = OversampledSoftClipper::new(true);
    let mut max_out = 0.0_f32;

    // +20dB (amplitude 10.0) sine — must be limited to ~1.0
    for i in 0..2000 {
        let t = i as f32 / 48000.0_f32;
        let sig = (t * 2.0_f32 * std::f32::consts::PI
                   * 100.0_f32).sin() * 10.0_f32;
        let (l_out, _) = clipper.process(sig, sig);
        if l_out.abs() > max_out { max_out = l_out.abs(); }
    }
    println!("Max output after limiting: {:.6}", max_out);
    // Gibbs phenomenon allows slight overshoot up to 1.05
    assert!(max_out <= 1.05_f32,
        "Clipper leaked massive signal! Max: {:.6}", max_out);
    assert!(max_out > 0.9_f32,
        "Clipper killed signal too much! Max: {:.6}", max_out);
}

#[test]
fn soft_clipper_bypass_is_transparent() {
    use sp314_dsp::limiter::OversampledSoftClipper;
    let mut clipper = OversampledSoftClipper::new(false);

    // With enabled=false, output must equal input exactly — zero latency
    let (l_out, r_out) = clipper.process(0.7_f32, -0.3_f32);
    assert_eq!(l_out,  0.7_f32,  "Bypass changed L");
    assert_eq!(r_out, -0.3_f32,  "Bypass changed R");
}
