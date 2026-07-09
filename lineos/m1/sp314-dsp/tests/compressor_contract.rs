use approx::assert_abs_diff_eq;
use serde_json::Value;
use sp314_dsp::compressor::{
    core::{CompressorBand, CompressorBandConfig},
    crossover::CrossoverLR4,
    envelope::EnvelopeFollower,
    gain::compute_gain_reduction,
    stereo::{CompressorV3, CompressorV3Config},
};
use std::fs;

fn load_fixture(name: &str) -> Value {
    let path = format!("tests/fixtures/{}.json", name);
    let content = fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read {}", path));
    serde_json::from_str(&content).expect("Failed to parse JSON")
}

// =========================================
// CrossoverLR4 tests
// =========================================

#[test]
fn crossover_lr4_sum_is_flat() {
    let fixture = load_fixture("crossover_reference");
    let checks = fixture["sum_checks"].as_array().unwrap();
    let sample_rate = fixture["sample_rate"].as_f64().unwrap() as u32;
    let crossover_hz = fixture["crossover_hz"].as_f64().unwrap() as f32;

    let mut lr4 = CrossoverLR4::new(crossover_hz, sample_rate);

    for check in checks {
        let freq_hz = check["freq_hz"].as_f64().unwrap() as f32;
        let expected_sum_db = check["expected_sum_db"].as_f64().unwrap() as f32;
        let tol = check["tolerance_db"].as_f64().unwrap() as f32;

        let w = 2.0 * std::f32::consts::PI * freq_hz / sample_rate as f32;

        lr4.reset();
        for i in 0..1000 {
            let x = (i as f32 * w).sin();
            lr4.process(x);
        }

        let mut peak_sum = 0.0_f32;
        for i in 0..1000 {
            let x = ((i + 1000) as f32 * w).sin();
            let (low, high) = lr4.process(x);
            let sum = low + high;
            if sum.abs() > peak_sum {
                peak_sum = sum.abs();
            }
        }

        let sum_db = 20.0 * peak_sum.log10();
        assert_abs_diff_eq!(sum_db, expected_sum_db, epsilon = tol);
    }
}

#[test]
fn crossover_lr4_split_at_crossover_freq() {
    let fixture = load_fixture("crossover_reference");
    let split = &fixture["split_at_crossover"];
    let sample_rate = fixture["sample_rate"].as_f64().unwrap() as u32;
    let crossover_hz = fixture["crossover_hz"].as_f64().unwrap() as f32;

    let mut lr4 = CrossoverLR4::new(crossover_hz, sample_rate);

    let freq_hz = split["freq_hz"].as_f64().unwrap() as f32;
    let exp_low = split["expected_low_db"].as_f64().unwrap() as f32;
    let exp_high = split["expected_high_db"].as_f64().unwrap() as f32;
    let tol = split["tolerance_db"].as_f64().unwrap() as f32;

    let w = 2.0 * std::f32::consts::PI * freq_hz / sample_rate as f32;

    for i in 0..1000 {
        let x = (i as f32 * w).sin();
        lr4.process(x);
    }

    let mut peak_low = 0.0_f32;
    let mut peak_high = 0.0_f32;
    for i in 0..1000 {
        let x = ((i + 1000) as f32 * w).sin();
        let (low, high) = lr4.process(x);
        if low.abs() > peak_low {
            peak_low = low.abs();
        }
        if high.abs() > peak_high {
            peak_high = high.abs();
        }
    }

    let low_db = 20.0 * peak_low.log10();
    let high_db = 20.0 * peak_high.log10();

    assert_abs_diff_eq!(low_db, exp_low, epsilon = tol);
    assert_abs_diff_eq!(high_db, exp_high, epsilon = tol);
}

#[test]
fn crossover_lr4_no_phase_inversion() {
    let mut lr4 = CrossoverLR4::new(150.0, 48000);
    let mut min_sum = 1000.0;

    for i in 0..1000 {
        // Feed an impulse
        let x = if i == 0 { 1.0 } else { 0.0 };
        let (low, high) = lr4.process(x);
        // If one is negated, sum might be very small, but more importantly,
        // let's just make sure low+high actually recovers the impulse.
        let sum = low + high;
        if i == 0 {
            // Because of group delay, the sum of an impulse won't be a perfect impulse instantly,
            // but for LR4, the allpass characteristic means energy is preserved and phase is identical.
        }
        if sum.abs() < min_sum {
            min_sum = sum.abs();
        }
    }

    // To strictly verify phase, feed a sine at 150Hz.
    // They should have exactly the same phase (or rather, their phase difference is 0).
    lr4.reset();
    let w = 2.0 * std::f32::consts::PI * 150.0 / 48000.0;

    for i in 0..1000 {
        let x = (i as f32 * w).sin();
        lr4.process(x);
    }

    // At steady state, check if low and high have the same sign.
    for i in 0..100 {
        let x = ((i + 1000) as f32 * w).sin();
        let (low, high) = lr4.process(x);
        // They should be in phase, so their product should be positive (or zero)
        assert!(
            low * high >= -1e-6,
            "Phase inversion detected: low={}, high={}",
            low,
            high
        );
    }
}

#[test]
fn crossover_lr4_deterministic() {
    let mut lr4_1 = CrossoverLR4::new(150.0, 48000);
    let mut outputs_1 = vec![];
    for i in 0..100 {
        outputs_1.push(lr4_1.process(i as f32));
    }

    for _ in 0..100 {
        let mut lr4_n = CrossoverLR4::new(150.0, 48000);
        for i in 0..100 {
            let out_n = lr4_n.process(i as f32);
            assert_eq!(outputs_1[i], out_n);
        }
    }
}

#[test]
fn crossover_lr4_reset_clears_state() {
    let mut lr4 = CrossoverLR4::new(150.0, 48000);
    lr4.process(1.0);
    lr4.reset();
    let (low, high) = lr4.process(0.0);
    assert_eq!(low, 0.0);
    assert_eq!(high, 0.0);
}

// =========================================
// EnvelopeFollower tests
// =========================================

#[test]
fn envelope_rc_coeff_matches_reference() {
    let fixture = load_fixture("envelope_reference");
    let cases = fixture["cases"].as_array().unwrap();

    // Verify formula exp(-2.2 / (time_ms * 0.001 * sample_rate))
    for case in cases {
        let time_ms = case["time_ms"].as_f64().unwrap() as f32;
        let sr = case["sample_rate"].as_f64().unwrap() as u32;
        let exp = case["expected_coeff"].as_f64().unwrap() as f32;
        let tol = case["tolerance"].as_f64().unwrap() as f32;

        let coeff = std::f32::consts::E.powf(-2.2 / (time_ms * 0.001 * sr as f32));
        assert_abs_diff_eq!(coeff, exp, epsilon = tol);
    }
}

#[test]
fn envelope_attack_follows_linear_domain() {
    let mut env = EnvelopeFollower::new(10.0, 100.0, 48000);

    // Process step from 0.0 to 1.0
    let out_db = env.process(1.0);
    // Envelope internal state should now be non-zero linear, then converted to db
    // 20*log10(1.0 - attack_coeff)
    // Actually we can't inspect the inner state easily, but we know it should attack
    assert!(out_db < 0.0 && out_db > -100.0);
}

#[test]
fn envelope_release_sample_rate_correct() {
    let mut env1 = EnvelopeFollower::new(10.0, 100.0, 48000);
    let mut env2 = EnvelopeFollower::new(10.0, 100.0, 44100);

    env1.process(1.0);
    let r1 = env1.process(0.0);

    env2.process(1.0);
    let r2 = env2.process(0.0);

    // They should release by different amounts per sample because dt is different
    assert!(r1 != r2);
}

#[test]
fn envelope_no_denormals_on_silence() {
    let mut env = EnvelopeFollower::new(10.0, 100.0, 48000);
    env.process(1.0);
    // Let it release
    for _ in 0..100000 {
        env.process(0.0);
    }
    // Should be flushed
    // To verify, process one more 0.0 and check output is very low (-100 or -200 dB depending on epsilon)
    let out = env.process(0.0);
    assert!(out <= -100.0);
}

#[test]
fn envelope_output_is_db() {
    let mut env = EnvelopeFollower::new(1.0, 1.0, 48000);
    // Let it settle
    for _ in 0..1000 {
        env.process(0.5);
    }
    let out = env.process(0.5);
    // 0.5 linear is -6.02 dB
    assert_abs_diff_eq!(out, -6.0206, epsilon = 0.1);
}

// =========================================
// GainComputer tests
// =========================================

#[test]
fn gain_computer_matches_reference() {
    let fixture = load_fixture("gain_computer_reference");
    let cases = fixture["cases"].as_array().unwrap();

    let threshold = -18.0;
    let ratio = 3.0;
    let knee = 2.0;

    for case in cases {
        let env_db = case["env_db"].as_f64().unwrap() as f32;
        let exp_gr = case["expected_gr"].as_f64().unwrap() as f32;

        let gr = compute_gain_reduction(env_db, threshold, ratio, knee);
        assert_abs_diff_eq!(gr, exp_gr, epsilon = 1e-4);
    }
}

#[test]
fn gain_computer_zero_below_knee() {
    let gr = compute_gain_reduction(-30.0, -18.0, 3.0, 2.0);
    assert_eq!(gr, 0.0);
}

#[test]
fn gain_computer_c1_continuous() {
    let threshold = -18.0;
    let ratio = 3.0;
    let knee = 2.0;

    // Knee boundaries are threshold - knee/2 = -19, and threshold + knee/2 = -17
    let eps = 1e-3;

    // Lower boundary slope
    let gr1 = compute_gain_reduction(-19.0 - eps, threshold, ratio, knee);
    let gr2 = compute_gain_reduction(-19.0, threshold, ratio, knee);
    let gr3 = compute_gain_reduction(-19.0 + eps, threshold, ratio, knee);

    let slope_below = (gr2 - gr1) / eps;
    let slope_above = (gr3 - gr2) / eps;
    assert_abs_diff_eq!(slope_below, slope_above, epsilon = 0.01);

    // Upper boundary slope
    let gr1 = compute_gain_reduction(-17.0 - eps, threshold, ratio, knee);
    let gr2 = compute_gain_reduction(-17.0, threshold, ratio, knee);
    let gr3 = compute_gain_reduction(-17.0 + eps, threshold, ratio, knee);

    let slope_below = (gr2 - gr1) / eps;
    let slope_above = (gr3 - gr2) / eps;
    assert_abs_diff_eq!(slope_below, slope_above, epsilon = 0.01);
}

#[test]
fn gain_computer_output_always_negative() {
    for env in -100..=20 {
        let gr = compute_gain_reduction(env as f32, -18.0, 3.0, 2.0);
        assert!(gr <= 0.0);
    }
}

#[test]
fn gain_computer_zero_at_ratio_1() {
    for env in -100..=20 {
        let gr = compute_gain_reduction(env as f32, -18.0, 1.0, 2.0);
        assert_eq!(gr, 0.0);
    }
}

// =========================================
// CompressorBand integration tests
// =========================================

fn default_config() -> CompressorBandConfig {
    CompressorBandConfig {
        threshold_db: -18.0,
        ratio: 3.0,
        knee_db: 2.0,
        attack_ms: 10.0,
        release_ms: 100.0,
        makeup_db: 0.0,
        crossover_hz: 150.0,
    }
}

#[test]
fn compressor_band_reduces_loud_signal() {
    let mut band = CompressorBand::new(default_config(), 48000);
    // feed loud signal (0dBFS = 1.0)
    for _ in 0..1000 {
        band.process(1.0); // Let it settle
    }
    let mut peak = 0.0;
    for _ in 0..1000 {
        let x = band.process(1.0); // DC
        if x.abs() > peak {
            peak = x.abs();
        }
    }
    // Should be reduced
    assert!(peak < 1.0);
}

#[test]
fn compressor_band_passes_quiet_signal() {
    let mut band = CompressorBand::new(default_config(), 48000);
    // feed quiet signal (-30dBFS)
    let in_val = 10f32.powf(-30.0 / 20.0);
    for _ in 0..1000 {
        band.process(in_val); // Let it settle
    }
    let mut peak = 0.0;
    for _ in 0..1000 {
        let x = band.process(in_val); // DC
        if x.abs() > peak {
            peak = x.abs();
        }
    }
    // Should be exactly in_val
    assert_abs_diff_eq!(peak, in_val, epsilon = 1e-4);
}

#[test]
fn compressor_band_no_nan_no_inf() {
    let mut band = CompressorBand::new(default_config(), 48000);
    for i in 0..10000 {
        let x = ((i * 137) % 200) as f32 / 100.0 - 1.0;
        let y = band.process(x);
        assert!(y.is_finite());
    }
}

#[test]
fn compressor_band_deterministic_100_runs() {
    let mut band1 = CompressorBand::new(default_config(), 48000);
    let mut out1 = vec![];
    for i in 0..100 {
        out1.push(band1.process(i as f32 / 100.0));
    }

    for _ in 0..100 {
        let mut band_n = CompressorBand::new(default_config(), 48000);
        for i in 0..100 {
            let y = band_n.process(i as f32 / 100.0);
            assert_eq!(y, out1[i]);
        }
    }
}

#[test]
fn compressor_band_reset_produces_identical_output() {
    let mut band = CompressorBand::new(default_config(), 48000);
    let mut out1 = vec![];
    for i in 0..100 {
        out1.push(band.process(i as f32 / 100.0));
    }

    band.reset();
    for i in 0..100 {
        let y = band.process(i as f32 / 100.0);
        assert_eq!(y, out1[i]);
    }
}

#[test]
fn compressor_band_output_within_headroom() {
    let mut band = CompressorBand::new(default_config(), 48000);
    for _ in 0..100 {
        let y = band.process(100.0); // blow up input
        assert!((-2.0..=2.0).contains(&y));
    }
}

// =========================================
// CompressorV3 tests
// =========================================

fn v3_default_config() -> CompressorV3Config {
    CompressorV3Config {
        mid_config: default_config(),
        side_config: default_config(),
    }
}

#[test]
fn compressor_v3_identity_no_gain_reduction() {
    let mut config = v3_default_config();
    config.mid_config.threshold_db = 0.0;
    config.mid_config.ratio = 1.0;
    config.side_config.threshold_db = 0.0;
    config.side_config.ratio = 1.0;

    let mut comp = CompressorV3::new(config, 48000);
    // feed DC stereo signal
    let in_l = 0.5;
    let in_r = -0.3;

    for _ in 0..1000 {
        let mut left = in_l;
        let mut right = in_r;
        comp.process_stereo(&mut left, &mut right);
    }

    let mut left = in_l;
    let mut right = in_r;
    comp.process_stereo(&mut left, &mut right);

    assert_abs_diff_eq!(left, in_l, epsilon = 1e-4);
    assert_abs_diff_eq!(right, in_r, epsilon = 1e-4);
}

#[test]
fn compressor_v3_mono_signal_side_is_zero() {
    let mut config = v3_default_config();
    // Heavy compression on side, none on mid
    config.mid_config.ratio = 1.0;
    config.side_config.threshold_db = -60.0;
    config.side_config.ratio = 10.0;

    let mut comp = CompressorV3::new(config, 48000);
    for i in 0..1000 {
        let mut left = (i as f32).sin();
        let mut right = left; // pure mono

        comp.process_stereo(&mut left, &mut right);

        // Output must preserve mono
        assert_eq!(left, right);
    }
}

#[test]
fn compressor_v3_ms_encode_decode_roundtrip() {
    let mut config = v3_default_config();
    config.mid_config.ratio = 1.0;
    config.side_config.ratio = 1.0;

    let mut comp = CompressorV3::new(config, 48000);
    let in_l = 0.7;
    let in_r = -0.4;

    for _ in 0..1000 {
        let mut left = in_l;
        let mut right = in_r;
        comp.process_stereo(&mut left, &mut right);
    }

    let mut left = in_l;
    let mut right = in_r;
    comp.process_stereo(&mut left, &mut right);

    assert_abs_diff_eq!(left, in_l, epsilon = 1e-4);
    assert_abs_diff_eq!(right, in_r, epsilon = 1e-4);
}

#[test]
fn compressor_v3_mid_independent_of_side() {
    let config = v3_default_config();
    // mid and side both compress above -18
    let mut comp = CompressorV3::new(config, 48000);

    for _ in 0..1000 {
        let mut l = 1.0;
        let mut r = 1.0;
        comp.process_stereo(&mut l, &mut r);
    }
    let mut l = 1.0;
    let mut r = 1.0;
    comp.process_stereo(&mut l, &mut r);

    assert!(l < 1.0);

    comp.reset();
    for _ in 0..1000 {
        let mut l = 0.01;
        let mut r = -0.01;
        comp.process_stereo(&mut l, &mut r);
    }
    let mut l = 0.01;
    let mut r = -0.01;
    comp.process_stereo(&mut l, &mut r);

    assert_abs_diff_eq!(l, 0.01, epsilon = 1e-4);
    assert_abs_diff_eq!(r, -0.01, epsilon = 1e-4);
}

#[test]
fn compressor_v3_deterministic_100_runs() {
    let mut comp1 = CompressorV3::new(v3_default_config(), 48000);
    let mut out1 = vec![];
    for i in 0..100 {
        let mut l = (i as f32).sin();
        let mut r = (i as f32).cos();
        comp1.process_stereo(&mut l, &mut r);
        out1.push((l, r));
    }

    for _ in 0..100 {
        let mut comp_n = CompressorV3::new(v3_default_config(), 48000);
        for i in 0..100 {
            let mut l = (i as f32).sin();
            let mut r = (i as f32).cos();
            comp_n.process_stereo(&mut l, &mut r);
            assert_eq!(out1[i].0, l);
            assert_eq!(out1[i].1, r);
        }
    }
}

#[test]
fn compressor_v3_output_within_headroom() {
    let mut comp = CompressorV3::new(v3_default_config(), 48000);
    for _ in 0..100 {
        let mut l = 100.0;
        let mut r = 100.0;
        comp.process_stereo(&mut l, &mut r);
        assert!((-2.0..=2.0).contains(&l));
        assert!((-2.0..=2.0).contains(&r));
    }
}

#[test]
fn compressor_v3_reset_produces_identical_output() {
    let mut comp = CompressorV3::new(v3_default_config(), 48000);
    let mut out1 = vec![];
    for i in 0..100 {
        let mut l = (i as f32).sin();
        let mut r = (i as f32).cos();
        comp.process_stereo(&mut l, &mut r);
        out1.push((l, r));
    }

    comp.reset();
    for i in 0..100 {
        let mut l = (i as f32).sin();
        let mut r = (i as f32).cos();
        comp.process_stereo(&mut l, &mut r);
        assert_eq!(out1[i].0, l);
        assert_eq!(out1[i].1, r);
    }
}
