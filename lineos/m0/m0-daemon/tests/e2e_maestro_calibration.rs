//! Maestro Calibration Test
//! Validates BPM thresholds (<100, <=130, >130) for ducking gain.
//! Authority: maestro-controller-spec-v1_0.md

use m0d::dsp::maestro::AutoTuningController;
use lineos_types::pre_analysis::PreAnalysisData;

#[test]
fn maestro_calibration_bpm_thresholds() {
    // 1. BPM < 100
    let mut pre_a = PreAnalysisData::silent();
    pre_a.bpm = 90.0;
    pre_a.transient_density = 0.5;
    let params_a = AutoTuningController::compute_render_params(&pre_a);

    // 2. BPM <= 130
    let mut pre_b = PreAnalysisData::silent();
    pre_b.bpm = 120.0;
    let params_b = AutoTuningController::compute_render_params(&pre_b);

    // 3. BPM > 130
    let mut pre_c = PreAnalysisData::silent();
    pre_c.bpm = 150.0;
    let params_c = AutoTuningController::compute_render_params(&pre_c);

    println!(
        "Track A (90 BPM): ducking_gain={:.3}, release_ms={:.1}",
        params_a.ducking_gain, params_a.release_ms
    );
    println!(
        "Track B (120 BPM): ducking_gain={:.3}, release_ms={:.1}",
        params_b.ducking_gain, params_b.release_ms
    );
    println!(
        "Track C (150 BPM): ducking_gain={:.3}, release_ms={:.1}",
        params_c.ducking_gain, params_c.release_ms
    );

    // Assert ranges
    assert!(
        params_a.ducking_gain >= 0.3 && params_a.ducking_gain <= 0.5,
        "Gate Failed: ducking_gain for <100 BPM should be [0.3, 0.5]"
    );
    assert!(
        (params_b.ducking_gain - 0.55).abs() < 0.001,
        "Gate Failed: ducking_gain for <=130 BPM should be 0.55"
    );
    assert!(
        (params_c.ducking_gain - 0.75).abs() < 0.001,
        "Gate Failed: ducking_gain for >130 BPM should be 0.75"
    );
}
