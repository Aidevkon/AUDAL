use sp314_dsp::pipeline::autotune::*;

#[test]
fn autotuner_transparent_returns_zero_immediately() {
    let result = autotune(-14.0, -14.0);
    assert_eq!(result.pre_gain_db, 0.0);
    assert_eq!(result.estimated_input_lufs, -14.0);
}

#[test]
fn autotuner_calculates_gain_correctly() {
    // Target is -14.0 LUFS, measured is -18.0 LUFS -> needs +4dB
    let result = autotune(-18.0, -14.0);
    assert_eq!(result.pre_gain_db, 4.0);
}

#[test]
fn autotuner_respects_clamping_limit() {
    // Target is -14.0 LUFS, measured is 20.0 LUFS -> needs -34dB, clamps to -20dB
    let result = autotune(20.0, -14.0);
    assert_eq!(result.pre_gain_db, -20.0);
    
    // Target is -14.0 LUFS, measured is -40.0 LUFS -> needs +26dB, clamps to +20dB
    let result = autotune(-40.0, -14.0);
    assert_eq!(result.pre_gain_db, 20.0);
}

#[test]
fn autotuner_is_fully_deterministic() {
    let result1 = autotune(-15.0, -14.0);
    let result2 = autotune(-15.0, -14.0);
    
    assert_eq!(result1.pre_gain_db, result2.pre_gain_db);
    assert_eq!(result1.estimated_input_lufs, result2.estimated_input_lufs);
}

#[test]
fn autotuner_chunk_finds_loudest_section() {
    // 6 seconds buffer @ 48kHz
    let len = 48000 * 6;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    
    // First 2s quiet
    for i in 0..(48000*2) {
        left[i] = 0.1; right[i] = 0.1;
    }
    // Middle 2s loud
    for i in (48000*2)..(48000*4) {
        left[i] = 0.5; right[i] = 0.5;
    }
    // Last 2s quiet
    for i in (48000*4)..len {
        left[i] = 0.1; right[i] = 0.1;
    }

    let start = find_highest_energy_chunk(&left, &right);
    // Should be exactly 2 seconds in (index 96000)
    assert_eq!(start, 96000);
}
