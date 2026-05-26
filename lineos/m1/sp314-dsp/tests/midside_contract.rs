#[test]
fn midside_preserves_mono() {
    use sp314_dsp::limiter::MidSideProcessor;
    let mut ms = MidSideProcessor::new();
    // Mono (L=R) → Side=0 → filter does nothing
    let (l_out, r_out) = ms.process(0.5_f32, 0.5_f32);
    assert!((l_out - 0.5_f32).abs() < 1e-6,
        "Mono L changed: {}", l_out);
    assert!((r_out - 0.5_f32).abs() < 1e-6,
        "Mono R changed: {}", r_out);
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
    println!("DC after 2000 samples: L={:.2e} R={:.2e}",
             l_out, r_out);
    assert!(l_out.abs() < 1e-3,
        "DC not blocked: L={}", l_out);
    assert!(r_out.abs() < 1e-3,
        "DC not blocked: R={}", r_out);
}
