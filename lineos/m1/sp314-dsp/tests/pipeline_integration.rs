use sp314_dsp::pipeline::engine::Sp314MasteringEngine;
use sp314_dsp::pipeline::presets::MasteringTarget;
use rustfft::{FftPlanner, num_complex::Complex};

#[test]
fn test_no_clipping_on_max_boost_with_transient() {
    let target = MasteringTarget::AggressiveEDM;
    let config = target.engine_config(48000);
    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();
    
    let len = 48000;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    
    left[0] = 0.99;
    right[0] = 0.99;
    
    engine.process_offline(&mut left, &mut right);
    
    for i in 0..len {
        assert!(left[i].abs() <= sp314_dsp::limiter::DEFAULT_CEILING_LINEAR);
        assert!(right[i].abs() <= sp314_dsp::limiter::DEFAULT_CEILING_LINEAR);
    }
}

#[test]
fn test_no_artifacts_at_eq_block_boundaries() {
    let target = MasteringTarget::SpotifyV3;
    let config = target.engine_config(48000);
    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();
    
    let len = 48000;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    
    for i in 0..len {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 10.0 * t).sin() * 0.5;
        left[i] = s;
        right[i] = s;
    }
    
    engine.process_offline(&mut left, &mut right);
    
    let mut max_delta = 0.0_f32;
    for i in 1..len {
        let delta = (left[i] - left[i-1]).abs();
        if delta > max_delta {
            max_delta = delta;
        }
    }
    
    assert!(max_delta < 0.1, "Found discontinuity of {}", max_delta);
}

#[test]
fn test_reset_produces_identical_output() {
    let target = MasteringTarget::SpotifyV3;
    let config = target.engine_config(48000);
    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();
    
    let len = 256;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    for i in 0..len {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.5;
        left[i] = s;
        right[i] = s;
    }
    
    let mut l1 = left.clone();
    let mut r1 = right.clone();
    engine.process_offline(&mut l1, &mut r1);
    
    engine.reset();
    
    let mut l2 = left.clone();
    let mut r2 = right.clone();
    engine.process_offline(&mut l2, &mut r2);
    
    assert_eq!(l1, l2);
    assert_eq!(r1, r2);
}

#[test]
fn test_no_comb_filtering_in_parallel_topology() {
    let target = MasteringTarget::SpotifyV3;
    let mut config = target.engine_config(48000);
    config.parallel_mix = 0.5;
    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();
    
    let len = 4096;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    left[0] = 1.0;
    right[0] = 1.0;
    
    engine.process_offline(&mut left, &mut right);
    
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(len);
    
    let mut buffer: Vec<Complex<f32>> = left.iter()
        .map(|&x| Complex { re: x, im: 0.0 })
        .collect();
        
    fft.process(&mut buffer);
    
    for i in 1..len/2 {
        let mag = (buffer[i].re * buffer[i].re + buffer[i].im * buffer[i].im).sqrt();
        assert!(mag >= 0.01, "Bin {} has magnitude {}", i, mag);
    }
}
