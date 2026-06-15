//! Maestro E2E Proof Test
//! Proves: low BPM track gets more aggressive ducking than fast track.
//! Authority: maestro-controller-spec-v1_0.md

use m0d::dsp::maestro::AutoTuningController;
use lineos_types::pre_analysis::PreAnalysisData;

/// Proof: Maestro applies more aggressive ducking to low BPM tracks.
#[test]
fn maestro_more_aggressive_ducking_for_low_bpm() {
    let mut pre_a = PreAnalysisData::silent();
    pre_a.bpm = 70.0;
    pre_a.transient_density = 0.5;

    let mut pre_b = PreAnalysisData::silent();
    pre_b.bpm = 140.0;

    let params_a = AutoTuningController::compute_render_params(&pre_a);
    let params_b = AutoTuningController::compute_render_params(&pre_b);

    println!(
        "Track A (70 BPM): ducking_gain={:.3}, release_ms={:.1}",
        params_a.ducking_gain, params_a.release_ms
    );
    println!(
        "Track B (140 BPM): ducking_gain={:.3}, release_ms={:.1}",
        params_b.ducking_gain, params_b.release_ms
    );

    // Track A has lower BPM → deeper ducking (lower ducking_gain value = more aggressive cut)
    assert!(
        params_a.ducking_gain < params_b.ducking_gain,
        "Low BPM track A should have deeper ducking_gain ({:.3}) < High BPM track B ({:.3})",
        params_a.ducking_gain,
        params_b.ducking_gain
    );

    println!("✅ Maestro proof: AI adapts ducking to rhythm/BPM");
}
