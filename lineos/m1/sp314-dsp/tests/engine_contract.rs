#![allow(deprecated)]

use approx::assert_abs_diff_eq;
use serde_json::Value;
use std::fs;

use sp314_dsp::compressor::core::CompressorBandConfig;
use sp314_dsp::compressor::stereo::CompressorV3Config;
use sp314_dsp::masking_eq::MaskingEQConfig;
use sp314_dsp::pipeline::engine::{calculate_adaptive_budget, EngineConfig, Sp314MasteringEngine};
use sp314_dsp::pipeline::telemetry::{analyze_offline_pre_pass, Telemetry};

fn load_fixture(name: &str) -> Value {
    let path = format!("tests/fixtures/{}.json", name);
    let content = fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read {}", path));
    serde_json::from_str(&content).expect("Failed to parse JSON")
}

fn default_engine_config() -> EngineConfig {
    let eq_config = MaskingEQConfig {
        target_db: [0.0; 8],
        mask_margin_db: 0.0,
        max_boost_db: 0.0,
        target_phon: 80.0,
    };
    let band = CompressorBandConfig {
        threshold_db: 0.0,
        ratio: 1.0,
        knee_db: 0.0,
        attack_ms: 10.0,
        release_ms: 100.0,
        makeup_db: 0.0,
        crossover_hz: 150.0,
    };
    let comp_config = CompressorV3Config {
        mid_config: band.clone(),
        side_config: band,
    };

    EngineConfig {
        eq_config,
        comp_config,
        parallel_mix: 0.5,
        target_makeup_db: 0.0,
        limiter_config: sp314_dsp::limiter::LimiterConfig::default(),
        restoration_config: sp314_dsp::restoration::RestorationConfig::bypass(),
        harmonic_config: None,
        clipper_enabled: false,
    }
}

#[test]
fn engine_telemetry_matches_reference() {
    let fixture = load_fixture("telemetry_reference");
    let cases = fixture["cases"].as_array().unwrap();
    let tol = fixture["tolerance_db"].as_f64().unwrap() as f32;

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expected_peak = case["peak_db"].as_f64().unwrap() as f32;
        let expected_rms = case["rms_db"].as_f64().unwrap() as f32;

        let mut left = vec![0.0_f32; 1024];
        let mut right = vec![0.0_f32; 1024];

        if name == "sine_0.5_1kHz_1024" {
            for i in 0..1024 {
                left[i] = 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * (i as f32) / 48000.0).sin();
                right[i] = left[i];
            }
        } else if name == "full_scale" {
            for i in 0..1024 {
                left[i] = 1.0;
                right[i] = 1.0;
            }
        }

        let tel = analyze_offline_pre_pass(&left, &right);

        assert_abs_diff_eq!(tel.peak_db, expected_peak, epsilon = tol);
        assert_abs_diff_eq!(tel.rms_db, expected_rms, epsilon = tol);
    }
}

#[test]
fn engine_adaptive_budget_selects_correct_pad() {
    let (pad, _) = calculate_adaptive_budget(
        &Telemetry {
            peak_db: -2.0,
            rms_db: -12.0,
            lufs: -144.0,
        },
        0.0,
    );
    assert_eq!(pad, -6.0);

    let (pad, _) = calculate_adaptive_budget(
        &Telemetry {
            peak_db: -5.0,
            rms_db: -15.0,
            lufs: -144.0,
        },
        0.0,
    );
    assert_eq!(pad, -6.0);

    let (pad, _) = calculate_adaptive_budget(
        &Telemetry {
            peak_db: -15.0,
            rms_db: -25.0,
            lufs: -144.0,
        },
        0.0,
    );
    assert_eq!(pad, -6.0);
}

#[test]
fn engine_parallel_blend_at_zero_percent_bypasses_compressor() {
    let mut config = default_engine_config();
    config.parallel_mix = 0.0;

    config.comp_config.mid_config.threshold_db = -60.0;
    config.comp_config.mid_config.ratio = 20.0;
    config.comp_config.side_config.threshold_db = -60.0;
    config.comp_config.side_config.ratio = 20.0;

    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();

    for _ in 0..60 {
        let mut left = vec![0.5; 1024];
        let mut right = vec![0.5; 1024];
        engine.process_offline(&mut left, &mut right);
    }

    let mut left = vec![0.5; 1024];
    let mut right = vec![0.5; 1024];
    engine.process_offline(&mut left, &mut right);

    assert_abs_diff_eq!(left[1023], 0.5, epsilon = 1e-3);
    assert_abs_diff_eq!(right[1023], 0.5, epsilon = 1e-3);
}

#[test]
fn engine_parallel_blend_at_100_percent_is_fully_wet() {
    let mut config = default_engine_config();
    config.parallel_mix = 1.0;

    config.comp_config.mid_config.threshold_db = -60.0;
    config.comp_config.mid_config.ratio = 20.0;

    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();

    for _ in 0..60 {
        let mut left = vec![0.5; 1024];
        let mut right = vec![0.5; 1024];
        engine.process_offline(&mut left, &mut right);
    }

    let mut left = vec![0.5; 1024];
    let mut right = vec![0.5; 1024];
    engine.process_offline(&mut left, &mut right);

    assert!(left[1023] < 0.5);
}

#[test]
fn engine_is_fully_deterministic() {
    let mut left_in = vec![0.0; 1024];
    let mut right_in = vec![0.0; 1024];
    for i in 0..1024 {
        left_in[i] = ((i % 30) as f32 / 15.0) - 1.0;
        right_in[i] = ((i % 40) as f32 / 20.0) - 1.0;
    }

    let config = default_engine_config();
    let mut engine1 = Sp314MasteringEngine::new(config.clone(), 48000).unwrap();

    let mut l1 = left_in.clone();
    let mut r1 = right_in.clone();
    engine1.process_offline(&mut l1, &mut r1);

    let mut engine2 = Sp314MasteringEngine::new(config.clone(), 48000).unwrap();
    let mut l2 = left_in.clone();
    let mut r2 = right_in.clone();
    engine2.process_offline(&mut l2, &mut r2);

    for i in 0..1024 {
        assert_eq!(l1[i], l2[i]);
        assert_eq!(r1[i], r2[i]);
    }
}

#[test]
fn engine_limiter_prevents_clipping() {
    let config = default_engine_config();
    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();
    let mut left = vec![2.0; 48000];
    let mut right = vec![2.0; 48000];
    engine.process_offline(&mut left, &mut right);

    let mut max_out = 0.0_f32;
    for i in 0..left.len() {
        if left[i].abs() > max_out {
            max_out = left[i].abs();
        }
        if right[i].abs() > max_out {
            max_out = right[i].abs();
        }
    }

    let ceiling = sp314_dsp::limiter::DEFAULT_CEILING_LINEAR;
    assert!(
        max_out <= ceiling + 1e-4,
        "Output peak {} exceeded ceiling {}",
        max_out,
        ceiling
    );
}

#[test]
fn engine_limiter_is_transparent_on_quiet_signal() {
    let mut config = default_engine_config();
    config.parallel_mix = 0.0;

    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();

    let mut left = vec![0.0; 48000];
    let mut right = vec![0.0; 48000];
    for i in 0..48000 {
        // -20 dBFS signal
        left[i] = 0.1 * (2.0 * std::f32::consts::PI * 1000.0 * (i as f32) / 48000.0).sin();
        right[i] = left[i];
    }

    let left_in = left.clone();

    engine.process_offline(&mut left, &mut right);

    // Check that signal is largely unchanged (RMS should be equal)
    let mut sum_sq_in = 0.0_f32;
    let mut sum_sq_out = 0.0_f32;
    for i in 1000..47000 {
        // ignore edges
        sum_sq_in += left_in[i] * left_in[i];
        sum_sq_out += left[i] * left[i];
    }

    let rms_in_db = 10.0 * (sum_sq_in / 46000.0).log10();
    let rms_out_db = 10.0 * (sum_sq_out / 46000.0).log10();

    assert_abs_diff_eq!(rms_out_db, rms_in_db, epsilon = 0.1);
}

#[test]
fn engine_harmonic_pipeline_end_to_end() {
    use sp314_dsp::harmonic::HarmonicConfig;

    let mut config = default_engine_config();
    config.harmonic_config = Some(HarmonicConfig {
        drive: 2.0,
        drive_compensation: 1.0, // engine.rs will override from pad_db
        even_amount: 0.6,
        odd_amount: 0.2,
        mix: 0.3,
    });
    config.parallel_mix = 1.0; // fully wet — ensures harmonics are in the output

    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();

    // 1kHz sine at -12 dBFS: amplitude = 10^(-12/20) ≈ 0.251
    let n = 48000_usize;
    let amp = libm::powf(10.0_f32, -12.0_f32 / 20.0_f32);
    let mut left = vec![0.0_f32; n];
    let mut right = vec![0.0_f32; n];
    for i in 0..n {
        let s = amp
            * libm::sinf(2.0_f32 * core::f32::consts::PI * 1000.0_f32 * (i as f32) / 48000.0_f32);
        left[i] = s;
        right[i] = s;
    }

    let left_in = left.clone();
    engine.process_offline(&mut left, &mut right);

    // 1. Output peak must be below 0 dBFS (no clipping)
    let max_out = left
        .iter()
        .chain(right.iter())
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max);
    assert!(max_out < 1.0_f32, "Output clipped: peak={:.4}", max_out);

    // 2. Output RMS within 6 dB of input RMS (gain staging sane)
    let margin = 2000_usize;
    let rms_in: f32 = left_in[margin..n - margin]
        .iter()
        .map(|s| s * s)
        .sum::<f32>()
        / (n - 2 * margin) as f32;
    let rms_out: f32 = left[margin..left.len() - margin]
        .iter()
        .map(|s| s * s)
        .sum::<f32>()
        / (left.len() - 2 * margin) as f32;
    let rms_in_db = 10.0_f32 * rms_in.max(1e-20).log10();
    let rms_out_db = 10.0_f32 * rms_out.max(1e-20).log10();
    assert_abs_diff_eq!(rms_out_db, rms_in_db, epsilon = 6.0_f32);

    // 3. Output is not identical to input (harmonics actually processed)
    let mut diff_sum = 0.0_f32;
    let compare_len = left_in.len().min(left.len());
    for i in margin..compare_len - margin {
        diff_sum += (left[i] - left_in[i]).abs();
    }
    assert!(
        diff_sum > 0.01_f32,
        "Output identical to input — harmonics not active. diff_sum={:.6}",
        diff_sum
    );
}

#[test]
fn engine_process_block_harmonic_no_overcook() {
    use sp314_dsp::harmonic::HarmonicConfig;

    let mut config = default_engine_config();
    config.harmonic_config = Some(HarmonicConfig {
        drive: 2.0,
        drive_compensation: 1.0, // will be set per-path by engine
        even_amount: 0.6,
        odd_amount: 0.2,
        mix: 0.3,
    });
    config.parallel_mix = 1.0; // fully wet

    let mut engine = Sp314MasteringEngine::new(config, 48000).unwrap();

    // 1kHz sine at 0.5 amplitude (unpadded signal level)
    let n = 4096_usize;
    let mut left = vec![0.0_f32; n];
    let mut right = vec![0.0_f32; n];
    for i in 0..n {
        let s = 0.5_f32
            * libm::sinf(2.0_f32 * core::f32::consts::PI * 1000.0_f32 * (i as f32) / 48000.0_f32);
        left[i] = s;
        right[i] = s;
    }

    let left_in = left.clone();
    engine.process_block(&mut left, &mut right);

    // 1. Output peak must be below 0 dBFS (no clipping from over-driven tanh)
    let max_out = left
        .iter()
        .chain(right.iter())
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max);
    assert!(
        max_out < 1.0_f32,
        "process_block output clipped: peak={:.4}",
        max_out
    );

    // 2. Output must not be identical to input (harmonics active)
    let margin = 100_usize;
    let mut diff_sum = 0.0_f32;
    for i in margin..n - margin {
        diff_sum += (left[i] - left_in[i]).abs();
    }
    assert!(
        diff_sum > 0.01_f32,
        "process_block output identical to input — harmonics not active"
    );

    // 3. RMS should be within 3 dB of input (no over-saturation)
    //    With drive_compensation=1.0, tanh(2.0*0.5)=tanh(1.0)≈0.76
    //    With the old bug (1.995), tanh(3.99*0.5)=tanh(2.0)≈0.96 — much hotter
    let rms_in: f32 = left_in[margin..n - margin]
        .iter()
        .map(|s| s * s)
        .sum::<f32>()
        / (n - 2 * margin) as f32;
    let rms_out: f32 =
        left[margin..n - margin].iter().map(|s| s * s).sum::<f32>() / (n - 2 * margin) as f32;
    let rms_in_db = 10.0_f32 * rms_in.max(1e-20).log10();
    let rms_out_db = 10.0_f32 * rms_out.max(1e-20).log10();
    assert_abs_diff_eq!(rms_out_db, rms_in_db, epsilon = 3.0_f32);
}
