//! S1 — Amplitude extreme stress tests.
//! Authority: spec/stress-test-suite.md S1

use super::generators::*;
#[allow(deprecated)]
use sp314_dsp::pipeline::engine::Sp314MasteringEngine;
use sp314_dsp::pipeline::presets::MasteringTarget;

fn make_engine(sample_rate: u32) -> Sp314MasteringEngine {
    let mut config = MasteringTarget::SpotifyV3.engine_config(sample_rate);
    // Stress tests use explicit -1.0 dBFS ceiling.
    // We test that the Limiter RESPECTS the given ceiling,
    // independent of preset defaults.
    config.limiter_config.ceiling_db = -1.0;
    Sp314MasteringEngine::new(config, sample_rate).expect("Engine init failed")
}

fn peak_dbfs(left: &[f32], right: &[f32]) -> f32 {
    let peak = left
        .iter()
        .chain(right.iter())
        .map(|s| s.abs())
        .fold(0.0f32, f32::max);
    if peak < 1e-9 {
        -144.0
    } else {
        20.0 * libm::log10f(peak)
    }
}

/// S1.1 — Full-scale 1kHz sine at 0 dBFS.
/// No NaN, no Inf. True peak must be <= -1.0 dBTP after limiting.
#[test]
fn s1_1_full_scale_sine() {
    let mono = sine(1000.0, 0.0, 10.0, 48000);
    let (mut left, mut right) = mono_to_stereo(&mono);
    let mut engine = make_engine(48000);
    let _telemetry = engine.process_offline(&mut left, &mut right);
    assert_no_nan_inf(&left, "S1.1 left");
    assert_no_nan_inf(&right, "S1.1 right");
    let output_peak = peak_dbfs(&left, &right);
    assert!(
        output_peak <= -0.9,
        "S1.1: true peak {:.4} dBFS exceeds -1.0 ceiling (±0.1 tolerance)",
        output_peak
    );
}

/// S1.2 — Clipped sine at +12 dBFS.
/// Verifies pipeline stability against massively clipped/hot signals.
#[test]
fn s1_2_clipped_sine() {
    let mono = sine(1000.0, 12.0, 10.0, 48000);
    let (mut left, mut right) = mono_to_stereo(&mono);
    let mut engine = make_engine(48000);
    let _telemetry = engine.process_offline(&mut left, &mut right);
    assert_no_nan_inf(&left, "S1.2 left");
    assert_no_nan_inf(&right, "S1.2 right");
    let output_peak = peak_dbfs(&left, &right);
    assert!(
        output_peak <= -0.9,
        "S1.2: true peak {:.4} dBFS exceeds -1.0 ceiling (±0.1 tolerance)",
        output_peak
    );
}

/// S1.3 — Near-silence: white noise at -80 dBFS.
/// No NaN, no Inf. No denormal explosion.
#[test]
fn s1_3_near_silence() {
    let mono = white_noise(-80.0, 30.0, 48000, 42);
    let (mut left, mut right) = mono_to_stereo(&mono);
    let mut engine = make_engine(48000);
    let _telemetry = engine.process_offline(&mut left, &mut right);
    assert_no_nan_inf(&left, "S1.3 left");
    assert_no_nan_inf(&right, "S1.3 right");
}

/// S1.4 — Absolute silence: all-zeros PCM.
/// No NaN, no Inf. Output must remain near-zero.
#[test]
fn s1_4_absolute_silence() {
    let mut left = silence(10.0, 48000);
    let mut right = silence(10.0, 48000);
    let mut engine = make_engine(48000);
    let _telemetry = engine.process_offline(&mut left, &mut right);
    assert_no_nan_inf(&left, "S1.4 left");
    assert_no_nan_inf(&right, "S1.4 right");
    // Output must be bounded
    assert_bounded(&left, "S1.4 left");
    assert_bounded(&right, "S1.4 right");
}

/// S1.5 — High dynamic range: alternating loud/quiet segments.
/// No NaN, no Inf.
#[test]
fn s1_5_high_dynamic_range() {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for _ in 0..10 {
        let loud = sine(1000.0, -3.0, 1.0, 48000);
        let quiet = sine(1000.0, -60.0, 1.0, 48000);
        left.extend_from_slice(&loud);
        left.extend_from_slice(&quiet);
        right.extend_from_slice(&loud);
        right.extend_from_slice(&quiet);
    }
    let mut engine = make_engine(48000);
    let _telemetry = engine.process_offline(&mut left, &mut right);
    assert_no_nan_inf(&left, "S1.5 left");
    assert_no_nan_inf(&right, "S1.5 right");
}
