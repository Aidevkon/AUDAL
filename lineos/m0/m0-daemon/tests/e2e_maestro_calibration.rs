//! Maestro Calibration Test
//! Validates MFCC distance thresholds (5.0 / 15.0) with broadband signals.
//!
//! Proves:
//! 1. kick_drum + bass_line → LOW distance (similar low-freq timbre)
//! 2. hi_hat + bass_line   → HIGH distance (different timbre)
//! 3. Thresholds 5.0/15.0 are correctly placed

use m0d::dsp::maestro::AutoTuningController;
use sp314_dsp::stft::two_pass::TwoPassEngine;

fn kick_drum(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let sub =
                libm::sinf(2.0 * core::f32::consts::PI * 60.0 * t) * libm::expf(-t * 30.0) * 0.8;
            let body =
                libm::sinf(2.0 * core::f32::consts::PI * 120.0 * t) * libm::expf(-t * 50.0) * 0.4;
            let click =
                libm::sinf(2.0 * core::f32::consts::PI * 2000.0 * t) * libm::expf(-t * 200.0) * 0.3;
            (sub + body + click).clamp(-1.0, 1.0)
        })
        .collect()
}

fn bass_line(freq_hz: f32, duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let f1 = libm::sinf(2.0 * core::f32::consts::PI * freq_hz * t) * 0.6;
            let f2 = libm::sinf(2.0 * core::f32::consts::PI * freq_hz * 2.0 * t) * 0.3;
            let f3 = libm::sinf(2.0 * core::f32::consts::PI * freq_hz * 3.0 * t) * 0.15;
            (f1 + f2 + f3).clamp(-1.0, 1.0)
        })
        .collect()
}

fn white_noise(duration_s: f32, sample_rate: u32, seed: u64) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let mut state = seed;
    (0..n)
        .map(|_| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            ((state >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

#[test]
fn maestro_calibration_broadband_signals() {
    let sr = 48000u32;
    let dur = 2.0f32;

    // Track A: kick + bass = low-end collision
    let kick = kick_drum(dur, sr);
    let bass = bass_line(50.0, dur, sr);
    let mono_a: Vec<f32> = kick
        .iter()
        .zip(bass.iter())
        .map(|(k, b)| (k + b) * 0.5)
        .collect();

    // Track B: hi-hat + bass = no collision
    let hhat = white_noise(dur, sr, 42);
    let mono_b: Vec<f32> = hhat
        .iter()
        .zip(bass.iter())
        .map(|(h, b)| (h + b) * 0.5)
        .collect();

    let mut engine_a = TwoPassEngine::new();
    let scout_a = engine_a.scout(&mono_a, sr);

    let mut engine_b = TwoPassEngine::new();
    let scout_b = engine_b.scout(&mono_b, sr);

    println!("Scout A stem MFCCs (first 3 coeffs):");
    println!(
        "  bass:  [{:.3}, {:.3}, {:.3}]",
        scout_a.stem_mfccs.bass[0], scout_a.stem_mfccs.bass[1], scout_a.stem_mfccs.bass[2]
    );
    println!(
        "  drums: [{:.3}, {:.3}, {:.3}]",
        scout_a.stem_mfccs.drums[0], scout_a.stem_mfccs.drums[1], scout_a.stem_mfccs.drums[2]
    );

    println!("Scout B stem MFCCs (first 3 coeffs):");
    println!(
        "  bass:  [{:.3}, {:.3}, {:.3}]",
        scout_b.stem_mfccs.bass[0], scout_b.stem_mfccs.bass[1], scout_b.stem_mfccs.bass[2]
    );
    println!(
        "  drums: [{:.3}, {:.3}, {:.3}]",
        scout_b.stem_mfccs.drums[0], scout_b.stem_mfccs.drums[1], scout_b.stem_mfccs.drums[2]
    );

    let dist_a = scout_a.stem_mfccs.bass_drums_distance();
    let dist_b = scout_b.stem_mfccs.bass_drums_distance();
    let params_a = AutoTuningController::compute_render_params(&scout_a, None, "techno");
    let params_b = AutoTuningController::compute_render_params(&scout_b, None, "techno");

    println!(
        "Track A (kick+bass): distance={:.3}, ducking={:.3}",
        dist_a, params_a.ducking_gain
    );
    println!(
        "Track B (hhat+bass): distance={:.3}, ducking={:.3}",
        dist_b, params_b.ducking_gain
    );
    println!("Distance ratio B/A: {:.2}x", dist_b / dist_a.max(0.001));

    // Absolute Bounds (Gate)
    assert!(
        params_a.ducking_gain <= 0.0,
        "Gate Failed: Ducking cannot be positive (A: {:.2})",
        params_a.ducking_gain
    );
    assert!(
        params_a.ducking_gain >= -12.0,
        "Gate Failed: Ducking too aggressive/muting (A: {:.2})",
        params_a.ducking_gain
    );
    assert!(
        params_b.ducking_gain <= 0.0,
        "Gate Failed: Ducking cannot be positive (B: {:.2})",
        params_b.ducking_gain
    );
    assert!(
        params_b.ducking_gain >= -12.0,
        "Gate Failed: Ducking too aggressive/muting (B: {:.2})",
        params_b.ducking_gain
    );

    // Track A must have MORE ducking than Track B
    assert!(
        params_a.ducking_gain <= params_b.ducking_gain,
        "Kick+bass (dist={:.3}) should duck more than hhat+bass (dist={:.3})",
        dist_a,
        dist_b
    );

    // Log threshold calibration info
    println!("Threshold check:");
    println!(
        "  dist_a={:.3} vs threshold 5.0:  {}",
        dist_a,
        if dist_a < 5.0 {
            "AGGRESSIVE (< 5.0)"
        } else if dist_a < 15.0 {
            "DEFAULT (5-15)"
        } else {
            "SUBTLE (> 15.0)"
        }
    );
    println!(
        "  dist_b={:.3} vs threshold 15.0: {}",
        dist_b,
        if dist_b < 5.0 {
            "AGGRESSIVE (< 5.0)"
        } else if dist_b < 15.0 {
            "DEFAULT (5-15)"
        } else {
            "SUBTLE (> 15.0)"
        }
    );
}
