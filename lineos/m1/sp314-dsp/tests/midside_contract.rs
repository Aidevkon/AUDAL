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
