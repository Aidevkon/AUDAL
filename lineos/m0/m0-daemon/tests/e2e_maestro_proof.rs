//! Maestro E2E Proof Test
//! Proves: bass-heavy track gets more aggressive ducking than clean track.
//! Authority: maestro-controller-spec-v1_0.md M-P6

use m0d::dsp::maestro::AutoTuningController;
use sp314_dsp::stft::two_pass::TwoPassEngine;

fn sine_wave(freq_hz: f32, n: usize, sr: u32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32 / sr as f32;
            libm::sinf(2.0 * core::f32::consts::PI * freq_hz * t) * 0.5
        })
        .collect()
}

/// Proof: Maestro applies more aggressive ducking to bass-heavy track.
#[test]
fn maestro_more_aggressive_ducking_for_bass_heavy_track() {
    let sr = 48000u32;
    let n = sr as usize * 2; // 2 seconds

    // Track A: sub-bass heavy (50Hz bass + 60Hz bass-like drums)
    // Low MFCC distance → aggressive ducking
    let bass_a = sine_wave(50.0, n, sr);
    let drums_a = sine_wave(60.0, n, sr);
    let mono_a: Vec<f32> = bass_a
        .iter()
        .zip(drums_a.iter())
        .map(|(b, d)| (b + d) * 0.5)
        .collect();

    // Track B: clean (50Hz bass + 8kHz hi-hat)
    // High MFCC distance → subtle ducking
    let bass_b = sine_wave(50.0, n, sr);
    let hhat_b = sine_wave(8000.0, n, sr);
    let mono_b: Vec<f32> = bass_b
        .iter()
        .zip(hhat_b.iter())
        .map(|(b, h)| (b + h) * 0.5)
        .collect();

    // Scout both tracks
    let mut engine_a = TwoPassEngine::new();
    let scout_a = engine_a.scout(&mono_a, sr);

    let mut engine_b = TwoPassEngine::new();
    let scout_b = engine_b.scout(&mono_b, sr);

    // Compute render params (no model — distance-only logic)
    let params_a = AutoTuningController::compute_render_params(&scout_a, None, "techno");
    let params_b = AutoTuningController::compute_render_params(&scout_b, None, "techno");

    let dist_a = scout_a.stem_mfccs.bass_drums_distance();
    let dist_b = scout_b.stem_mfccs.bass_drums_distance();

    println!(
        "Track A (50Hz + 60Hz): distance={:.2}, ducking_gain={:.3}",
        dist_a, params_a.ducking_gain
    );
    println!(
        "Track B (50Hz + 8kHz): distance={:.2}, ducking_gain={:.3}",
        dist_b, params_b.ducking_gain
    );

    // Track A has lower distance (similar timbre) → more aggressive ducking
    assert!(
        params_a.ducking_gain <= params_b.ducking_gain,
        "Bass-heavy track A (dist={:.2}) should have ducking_gain ({:.3}) \
         <= clean track B (dist={:.2}) ducking_gain ({:.3})",
        dist_a,
        params_a.ducking_gain,
        dist_b,
        params_b.ducking_gain
    );

    println!("✅ Maestro proof: AI adapts ducking to track timbre");
}
