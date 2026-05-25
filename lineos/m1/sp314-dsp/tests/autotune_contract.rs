use sp314_dsp::pipeline::autotune::*;
use sp314_dsp::pipeline::presets::MasteringTarget;

#[test]
fn autotuner_transparent_returns_zero_immediately() {
    let target = MasteringTarget::Transparent;
    let config = target.engine_config(48000);
    
    let left = vec![0.0; 48000];
    let right = vec![0.0; 48000];
    
    let result = autotune(&left, &right, config, target, 48000);
    assert_eq!(result.makeup_db, 0.0);
    assert_eq!(result.iterations, 0);
}

#[test]
fn autotuner_converges_to_target_lufs() {
    let target = MasteringTarget::SpotifyV3;
    let config = target.engine_config(48000);
    
    // Generate 1kHz sine at -18.0 dBFS RMS
    let target_rms_db = -18.0_f32;
    let target_rms_linear = 10.0_f32.powf(target_rms_db / 10.0).sqrt(); // Actually 10^(dB/20)
    let target_peak = target_rms_linear * std::f32::consts::SQRT_2;

    let len = 96000;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    for i in 0..len {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * target_peak;
        left[i] = s;
        right[i] = s;
    }

    let result = autotune(&left, &right, config, target, 48000);
    
    // Target is -14.0 LUFS
    assert!((result.achieved_lufs - (-14.0)).abs() < 0.1);
    assert!(result.converged);
    assert!(result.iterations <= 10);
}

#[test]
fn autotuner_respects_clipping_limit() {
    let target = MasteringTarget::AggressiveEDM;
    let config = target.engine_config(48000);
    
    // Very hot signal: square wave at -0.5 dBFS peak
    let len = 96000;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    let peak = 10.0_f32.powf(-0.5 / 20.0);
    for i in 0..len {
        let t = i as f32 / 48000.0;
        let s = if (2.0 * std::f32::consts::PI * 100.0 * t).sin() >= 0.0 { peak } else { -peak };
        left[i] = s;
        right[i] = s;
    }

    let result = autotune(&left, &right, config, target, 48000);
    
    // Should hit clipping ratio limit and back off
    assert!(result.clipping_ratio <= AUTOTUNE_MAX_CLIP_RATIO);
    // Might not converge due to clipping, but that's safe
}

#[test]
fn autotuner_is_fully_deterministic() {
    let target = MasteringTarget::SpotifyV3;
    let config = target.engine_config(48000);
    
    let len = 96000;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    for i in 0..len {
        left[i] = (i as f32).sin() * 0.5;
        right[i] = ((i + 1) as f32).cos() * 0.5;
    }

    let result1 = autotune(&left, &right, config.clone(), target, 48000);
    let result2 = autotune(&left, &right, config.clone(), target, 48000);
    
    assert_eq!(result1.makeup_db, result2.makeup_db);
    assert_eq!(result1.iterations, result2.iterations);
    assert_eq!(result1.achieved_lufs, result2.achieved_lufs);
    assert_eq!(result1.clipping_ratio, result2.clipping_ratio);
    assert_eq!(result1.converged, result2.converged);
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
