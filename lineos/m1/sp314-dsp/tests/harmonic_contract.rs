// tests/harmonic_contract.rs
// Acceptance tests for HarmonicEngine.
// Constitutional requirement: all math via libm.

use sp314_dsp::harmonic::{HarmonicEngine, HarmonicConfig};

#[test]
fn harmonic_at_mix_zero_is_dry() {
    let mut config = HarmonicConfig::default();
    config.mix = 0.0;
    let mut engine = HarmonicEngine::new(config);

    let (mid, side) = (0.5, -0.3);
    let (out_m, out_s) = engine.process_frame(mid, side);

    assert_eq!(out_m, mid);
    assert_eq!(out_s, side);
}

#[test]
fn harmonic_increases_rms_at_nonzero_drive() {
    let mut config = HarmonicConfig::default();
    config.drive = 6.0;
    let mut engine = HarmonicEngine::new(config);

    let mut sum_sq_in = 0.0;
    let mut sum_sq_out = 0.0;
    
    for i in 0..1000 {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.5;
        
        let (out, _) = engine.process_frame(s, 0.0);
        
        sum_sq_in += s * s;
        sum_sq_out += out * out;
    }
    
    let rms_in = (sum_sq_in / 1000.0).sqrt();
    let rms_out = (sum_sq_out / 1000.0).sqrt();
    
    assert!(rms_out > rms_in, "RMS out {} not > RMS in {}", rms_out, rms_in);
}

#[test]
fn harmonic_never_clips_within_range() {
    let mut config = HarmonicConfig::default();
    config.drive = 6.0;
    let mut engine = HarmonicEngine::new(config);

    for _ in 0..10000 {
        let (out_m, out_s) = engine.process_frame(1.0, 0.0);
        assert!(out_m >= -1.5 && out_m <= 1.5);
        assert!(out_s >= -1.5 && out_s <= 1.5);
    }
}

#[test]
fn harmonic_reset_produces_identical_output() {
    let config = HarmonicConfig::default();
    let mut engine = HarmonicEngine::new(config);

    let mut out1 = vec![];
    for i in 0..100 {
        let s = (i as f32 * 0.1).sin();
        out1.push(engine.process_frame(s, s));
    }

    engine.reset();

    let mut out2 = vec![];
    for i in 0..100 {
        let s = (i as f32 * 0.1).sin();
        out2.push(engine.process_frame(s, s));
    }

    assert_eq!(out1, out2);
}

#[test]
fn harmonic_adds_total_harmonic_distortion() {
    let config = HarmonicConfig {
        drive: 4.0,
        even_amount: 1.0,
        odd_amount: 1.0,
        mix: 1.0,
    };
    let mut engine = HarmonicEngine::new(config);

    let mut diff_sum_sq = 0.0;

    for i in 0..1000 {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.8;
        
        let (out, _) = engine.process_frame(s, 0.0);
        let diff = out - s;
        diff_sum_sq += diff * diff;
    }

    let diff_rms = (diff_sum_sq / 1000.0).sqrt();
    assert!(diff_rms > 0.01, "THD was too low: {}", diff_rms);
}
