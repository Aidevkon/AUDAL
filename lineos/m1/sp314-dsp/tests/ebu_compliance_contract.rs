use approx::assert_abs_diff_eq;
use sp314_dsp::metering::measure_integrated_lufs;
use std::f32::consts::PI;

/// EBU COMPLIANCE CONTRACT: RELATIVE GATING
///
/// This test verifies the core requirement of ITU-R BS.1770-4 / EBU R128 gating:
/// "The integrated loudness measurement must pause (gate out) when the signal drops
/// below -10 LU relative to the ungated absolute loudness."
///
/// We test this by creating a 9-second synthetic stereo signal in 3 phases:
/// - Phase 1 (0-3s): 1kHz sine wave at -20.0 dB amplitude.
/// - Phase 2 (3-6s): 1kHz sine wave at -40.0 dB amplitude (deep drop).
/// - Phase 3 (6-9s): 1kHz sine wave at -20.0 dB amplitude.
///
/// We choose a -40dB tone for Phase 2 instead of absolute digital silence (0.0).
/// Absolute silence works, but -40dB is a more robust test of the relative gate
/// because it guarantees we don't accidentally pass due to a `log10(0)` edge-case
/// or an absolute gate (-70 LUFS) short-circuit. A -40dB block has real energy,
/// but it falls below the relative gate threshold (-10 LU below the average).
///
/// If the gate works correctly, Phase 2 is entirely excluded, and the measured
/// LUFS of the 9-second track should be identical to the LUFS of the -20.0 dB
/// blocks alone. If the gate were failing (i.e. just an average of all blocks),
/// the -40dB phase would drag the final LUFS measurement significantly lower.
#[test]
fn test_ebu_gating_compliance() {
    let sr = 48000;
    let phase_duration_samples = sr * 3; // 3 seconds per phase

    let mut left = Vec::with_capacity(phase_duration_samples * 3);
    let mut right = Vec::with_capacity(phase_duration_samples * 3);

    let freq = 1000.0;
    let phase_1_3_amp = 10_f32.powf(-20.0 / 20.0);
    let phase_2_amp = 10_f32.powf(-40.0 / 20.0);

    // Helper to generate a continuous sine wave block
    let mut add_sine = |amp: f32, start_sample: usize| {
        for i in 0..phase_duration_samples {
            let t = (start_sample + i) as f32 / sr as f32;
            let sample = (2.0 * PI * freq * t).sin() * amp;
            left.push(sample);
            right.push(sample);
        }
    };

    // Phase 1 (0-3s): -20dB
    add_sine(phase_1_3_amp, 0);

    // Phase 2 (3-6s): -40dB (Should be gated out)
    add_sine(phase_2_amp, phase_duration_samples);

    // Phase 3 (6-9s): -20dB
    add_sine(phase_1_3_amp, phase_duration_samples * 2);

    let lufs = measure_integrated_lufs(&left, &right);

    // From our Python oracle, a -20dB sine yields approx -20.035 LUFS.
    // If the gating works, the final LUFS should be extremely close to -20.0
    // because Phase 2 is excluded. We use a tolerance of ±1.0 LU to account
    // for overlapping block boundary effects (400ms blocks over the phase transitions).
    assert_abs_diff_eq!(lufs, -20.0, epsilon = 1.0);
}
