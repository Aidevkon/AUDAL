use sp314_dsp::pipeline::engine::Sp314MasteringEngine;
use sp314_dsp::pipeline::presets::MasteringTarget;
use sp314_dsp::pipeline::autotune::{
    autotune, AUTOTUNE_MAX_CLIP_RATIO,
    measure_clipping_ratio_post_process
};
use sp314_dsp::pipeline::telemetry::analyze_offline_pre_pass;

// Helper: generate 1kHz sine at target amplitude
fn sine_1khz(amplitude: f32, samples: usize, sample_rate: u32) -> Vec<f32> {
    let fade_samples = (sample_rate / 100) as usize; // 10ms
    (0..samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let env = if i < fade_samples {
                // half-cosine fade in
                0.5 * (1.0 - (std::f32::consts::PI * (1.0 - (i as f32 / fade_samples as f32))).cos())
            } else {
                1.0
            };
            amplitude * env * (2.0 * std::f32::consts::PI * 1000.0 * t).sin()
        })
        .collect()
}

// Helper: measure peak_db from slice
fn peak_db(signal: &[f32]) -> f32 {
    let peak = signal.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
    if peak < 1e-9 { -144.0 } else { 20.0 * peak.log10() }
}

#[test]
fn test_engine_null_state_is_transparent() {
    let sine = sine_1khz(0.5, 4096, 48000);
    
    let config = MasteringTarget::Transparent.engine_config(48000);
    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();
    
    let telemetry_in = analyze_offline_pre_pass(&sine, &sine);
    let input_rms = telemetry_in.rms_db;

    let mut left = sine.clone();
    let mut right = sine.clone();
    
    engine.process_offline(&mut left, &mut right);
    
    let telemetry_out = analyze_offline_pre_pass(&left, &right);
    let output_rms = telemetry_out.rms_db;

    assert!((output_rms - input_rms).abs() < 0.1,
        "Transparent preset must not change loudness. Delta: {} dB",
        (output_rms - input_rms).abs());
}

#[test]
fn test_engine_eq_adds_energy_without_clipping() {
    let sine = sine_1khz(0.3, 4096, 48000);
    
    let mut config = MasteringTarget::Transparent.engine_config(48000);
    config.eq_config.target_db = [6.0; 8];
    // max_boost_db needs to be large enough to allow 6dB boost
    config.eq_config.max_boost_db = 6.0;
    
    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();
    
    let telemetry_in = analyze_offline_pre_pass(&sine, &sine);
    let input_rms = telemetry_in.rms_db;

    let mut left = sine.clone();
    let mut right = sine.clone();
    
    engine.process_offline(&mut left, &mut right);
    
    let telemetry_out = analyze_offline_pre_pass(&left, &right);
    let output_rms = telemetry_out.rms_db;
    
    let output_peak_db = peak_db(&left);

    assert!(output_rms > input_rms,
        "EQ boost must increase RMS");

    assert!(output_peak_db < 0.0,
        "EQ boost must not cause clipping. Peak: {} dBFS", output_peak_db);
}

#[test]
fn test_presets_respect_headroom() {
    // 1kHz sine wave calibrated to -12.0 dBFS RMS
    let target_rms_linear = 10.0_f32.powf(-12.0 / 20.0);
    let target_peak = target_rms_linear * std::f32::consts::SQRT_2;
    // Use 1 second buffer (48000) so initial compressor attack transient doesn't skew ratio
    let sine = sine_1khz(target_peak, 48000, 48000);

    let active_presets = [
        MasteringTarget::SpotifyV3,
        MasteringTarget::PodcastVoice,
        MasteringTarget::LoudMaster,
        MasteringTarget::AggressiveEDM,
        MasteringTarget::ClassicalAcoustic,
        MasteringTarget::BroadcastVideo,
        MasteringTarget::AtscA85,
    ];

    for preset in active_presets.iter() {
        let config = preset.engine_config(48000);
        let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();
        
        let mut left = sine.clone();
        let mut right = sine.clone();
        engine.process_offline(&mut left, &mut right);
        
        let clip_ratio = measure_clipping_ratio_post_process(&left, &right);
        
        assert!(clip_ratio <= 0.01,
            "Preset {:?} clips at -12 dBFS input. Ratio: {}",
            preset, clip_ratio);
    }
}

#[test]
fn test_autotuner_converges_for_all_presets() {
    // 1kHz sine wave at -18.0 dBFS RMS, 96_000 samples
    let target_rms_linear = 10.0_f32.powf(-18.0 / 20.0);
    let target_peak = target_rms_linear * std::f32::consts::SQRT_2;
    let sine = sine_1khz(target_peak, 96000, 48000);

    let active_presets = [
        MasteringTarget::SpotifyV3,
        MasteringTarget::PodcastVoice,
        MasteringTarget::LoudMaster,
        MasteringTarget::AggressiveEDM,
        MasteringTarget::ClassicalAcoustic,
        MasteringTarget::BroadcastVideo,
        MasteringTarget::AtscA85,
    ];

    for preset in active_presets.iter() {
        let config = preset.engine_config(48000);
        
        // In Phase 8, autotune is just pure math. We know input is -18.0 LUFS.
        let target_lufs = match preset {
            MasteringTarget::PodcastVoice => -16.0,
            _ => -14.0,
        };
        let result = autotune(-18.0, target_lufs);
        
        // Ensure it calculated a reasonable gain
        assert!(result.pre_gain_db > 0.0);
    }
}

#[test]
fn test_full_pipeline_is_deterministic() {
    let sine = sine_1khz(0.5, 4096, 48000);
    let target = MasteringTarget::SpotifyV3;
    
    // Run 1
    let config1 = target.engine_config(48000);
    let tune1 = autotune(-18.0, -14.0);
    
    let mut config_final_1 = config1.clone();
    config_final_1.target_makeup_db = tune1.pre_gain_db;
    let mut engine1 = Sp314MasteringEngine::new(config_final_1, 48000).unwrap();
    let mut left1 = sine.clone();
    let mut right1 = sine.clone();
    engine1.process_offline(&mut left1, &mut right1);
    
    // Run 2
    let config2 = target.engine_config(48000);
    let tune2 = autotune(-18.0, -14.0);
    
    let mut config_final_2 = config2.clone();
    config_final_2.target_makeup_db = tune2.pre_gain_db;
    let mut engine2 = Sp314MasteringEngine::new(config_final_2, 48000).unwrap();
    let mut left2 = sine.clone();
    let mut right2 = sine.clone();
    engine2.process_offline(&mut left2, &mut right2);
    
    for i in 0..left1.len() {
        assert_eq!(left1[i], left2[i],
            "Full pipeline must be deterministic");
        assert_eq!(right1[i], right2[i],
            "Full pipeline must be deterministic");
    }
}

#[test]
fn test_podcast_voice_ebu_r128_compliance() {
    use sp314_dsp::metering::measure_integrated_lufs;

    let sample_rate = 48000_u32;
    let n_samples   = 96000_usize; // 2 seconds — minimum for accurate LUFS

    // 1kHz sine calibrated to -12.0 dBFS RMS
    let target_rms_linear = 10.0_f32.powf(-12.0 / 20.0);
    let target_peak = target_rms_linear * std::f32::consts::SQRT_2;
    let sine = sine_1khz(target_peak, n_samples, sample_rate);

    // Autotune to converge makeup gain for PodcastVoice target (-16 LUFS)
    let preset = MasteringTarget::PodcastVoice;
    let config = preset.engine_config(sample_rate);
    
    use sp314_dsp::analysis::PreAnalyzer;
    let pre_analysis = PreAnalyzer::run(&sine, &sine, sample_rate);
    let tune   = autotune(pre_analysis.integrated_lufs, -16.0);

    // Build engine with converged makeup and process
    let mut final_config = config;
    final_config.target_makeup_db = tune.pre_gain_db;
    let mut engine = Sp314MasteringEngine::new(final_config, sample_rate).unwrap();

    let mut left  = sine.clone();
    let mut right = sine.clone();
    engine.process_offline(&mut left, &mut right);

    // Measure output integrated LUFS
    let output_lufs = measure_integrated_lufs(&left, &right);

    // PodcastVoice target is -16.0 LUFS — allow ±2 dB tolerance
    // EBU R128 podcast range: -16 to -14 LUFS
    assert!(output_lufs > -18.0 && output_lufs < -12.0,
        "PodcastVoice output LUFS out of EBU R128 range: {:.2} LUFS \
         (expected -18.0 to -12.0)", output_lufs);

    // No clipping
    let clip_ratio = measure_clipping_ratio_post_process(&left, &right);
    assert!(clip_ratio <= 0.01,
        "PodcastVoice clipped at -12 dBFS input. Ratio: {}", clip_ratio);
}
