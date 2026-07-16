// allow: contract tests index synthetic buffers the same way the
// DSP code under test does; iterator rewrites add nothing here.
#![allow(clippy::needless_range_loop)]

use approx::assert_abs_diff_eq;
use serde_json::Value;
use sp314_dsp::psychoacoustic::{
    bark::{bin_to_bark_band, calculate_mask_per_bin, spreading_attenuation_db, MPEG1_BARK_BANDS},
    iso226::interpolate_iso226_correction,
};
use std::fs;

fn load_fixture(name: &str) -> Value {
    let path = format!("tests/fixtures/{}.json", name);
    let content = fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read {}", path));
    serde_json::from_str(&content).expect("Failed to parse JSON")
}

// ==========================================
// ISO 226 tests
// ==========================================

#[test]
fn iso226_returns_exact_value_at_reference_points() {
    let fixture = load_fixture("iso226_reference");
    let freqs = fixture["freqs_hz"].as_array().unwrap();
    let corr_80 = fixture["80phon_correction_db"].as_array().unwrap();
    let corr_90 = fixture["90phon_correction_db"].as_array().unwrap();

    for i in 0..freqs.len() {
        let f = freqs[i].as_f64().unwrap() as f32;
        let c80 = corr_80[i].as_f64().unwrap() as f32;
        let c90 = corr_90[i].as_f64().unwrap() as f32;

        let res80 = interpolate_iso226_correction(f, 80.0);
        let res90 = interpolate_iso226_correction(f, 90.0);

        assert_abs_diff_eq!(res80, c80, epsilon = 1e-4);
        assert_abs_diff_eq!(res90, c90, epsilon = 1e-4);
    }
}

#[test]
fn iso226_c1_continuity_at_interior_points() {
    let fixture = load_fixture("iso226_reference");
    let freqs = fixture["freqs_hz"].as_array().unwrap();
    let tol = fixture["c1_continuity"]["tolerance_db_per_hz"]
        .as_f64()
        .unwrap() as f32;

    for i in 1..(freqs.len() - 1) {
        let f = freqs[i].as_f64().unwrap() as f32;

        let val_at_f = interpolate_iso226_correction(f, 80.0);
        let val_below = interpolate_iso226_correction(f - 0.5, 80.0);
        let val_above = interpolate_iso226_correction(f + 0.5, 80.0);

        let deriv_left = (val_at_f - val_below) / 0.5;
        let deriv_right = (val_above - val_at_f) / 0.5;

        assert_abs_diff_eq!(deriv_left, deriv_right, epsilon = tol);
    }
}

#[test]
fn iso226_phon_interpolation_valid_range() {
    let fixture = load_fixture("iso226_reference");
    let spots = fixture["interpolated_spot_checks"].as_array().unwrap();

    for check in spots {
        let f = check["freq_hz"].as_f64().unwrap() as f32;
        let p = check["phon"].as_f64().unwrap() as f32;
        let exp = check["expected_db"].as_f64().unwrap() as f32;
        let tol = check["tolerance"].as_f64().unwrap() as f32;

        let res = interpolate_iso226_correction(f, p);
        assert_abs_diff_eq!(res, exp, epsilon = tol);
    }
}

#[test]
fn iso226_boundary_clamping_low_frequency() {
    let val_5hz = interpolate_iso226_correction(5.0, 80.0);
    let val_20hz = interpolate_iso226_correction(20.0, 80.0);
    assert_eq!(
        val_5hz, val_20hz,
        "Low frequencies must clamp to 20Hz value"
    );
}

#[test]
fn iso226_boundary_clamping_high_frequency() {
    let val_20khz = interpolate_iso226_correction(20000.0, 80.0);
    let val_12_5khz = interpolate_iso226_correction(12500.0, 80.0);
    assert_eq!(
        val_20khz, val_12_5khz,
        "High frequencies must clamp to 12500Hz value"
    );
}

#[test]
fn iso226_no_panic_on_boundary_inputs() {
    // Should not panic or produce NaN/Infinity
    let res1 = interpolate_iso226_correction(-10.0, 80.0);
    assert!(res1.is_finite());
    let res2 = interpolate_iso226_correction(48000.0, 90.0);
    assert!(res2.is_finite());

    // Test phon clamping
    let res3 = interpolate_iso226_correction(1000.0, 50.0);
    let res4 = interpolate_iso226_correction(1000.0, 80.0);
    assert_eq!(res3, res4);

    let res5 = interpolate_iso226_correction(1000.0, 110.0);
    let res6 = interpolate_iso226_correction(1000.0, 90.0);
    assert_eq!(res5, res6);
}

// ==========================================
// Bark band tests
// ==========================================

#[test]
fn bark_bin_to_band_spot_checks() {
    let fixture = load_fixture("bark_reference");
    let spots = fixture["bin_to_band_checks"].as_array().unwrap();
    let sr = fixture["sample_rate_hz"].as_u64().unwrap() as u32;

    for check in spots {
        let fft = check["fft_size"].as_u64().unwrap() as usize;
        let bin = check["bin"].as_u64().unwrap() as usize;
        let exp = check["expected_band"].as_u64().unwrap() as usize;

        let res = bin_to_bark_band(bin, fft, sr);
        assert_eq!(res, exp, "bin_to_bark_band error at bin={}", bin);
    }
}

#[test]
fn bark_no_panic_on_any_valid_bin() {
    let fft_size = 1024;
    let sr = 48000;
    for bin in 0..=(fft_size / 2) {
        let _ = bin_to_bark_band(bin, fft_size, sr);
    }
}

#[test]
fn bark_result_always_in_valid_range() {
    let fft_size = 4096;
    let sr = 96000;
    for bin in 0..=(fft_size / 2) {
        let b = bin_to_bark_band(bin, fft_size, sr);
        assert!(b < MPEG1_BARK_BANDS, "Band {} is out of range", b);
    }
}

// ==========================================
// Spreading function tests
// ==========================================

#[test]
fn spreading_spot_checks() {
    let fixture = load_fixture("spreading_reference");
    let spots = fixture["spot_checks"].as_array().unwrap();

    for check in spots {
        if check.get("note_cap").is_some() {
            continue;
        }

        let dz = check["dz"].as_f64().unwrap() as f32;
        let exp = check["expected_db"].as_f64().unwrap() as f32;
        let tol = check["tolerance"].as_f64().unwrap() as f32;

        let res = spreading_attenuation_db(dz);
        assert_abs_diff_eq!(res, exp, epsilon = tol);
    }
}

#[test]
fn spreading_always_non_negative() {
    for i in 0..100 {
        let dz = i as f32 * 0.25;
        assert!(spreading_attenuation_db(dz) >= 0.0);
    }
}

#[test]
fn spreading_zero_at_zero_distance() {
    assert_eq!(spreading_attenuation_db(0.0), 0.0);
}

#[test]
fn spreading_monotonic_increasing() {
    for i in 0..80 {
        let dz1 = i as f32 * 0.1;
        let dz2 = dz1 + 0.05;
        let a1 = spreading_attenuation_db(dz1);
        let a2 = spreading_attenuation_db(dz2);
        assert!(a1 <= a2, "Spreading function must be monotonic increasing");
    }
}

#[test]
fn spreading_capped_at_noise_floor() {
    // dz=24 -> 87.0
    let res = spreading_attenuation_db(24.0);
    assert_abs_diff_eq!(res, 87.0, epsilon = 1e-4);
}

// ==========================================
// Integration test
// ==========================================

#[test]
fn mask_per_bin_no_panic_fft_sizes() {
    let sizes = [512, 1024, 2048, 4096];
    let sr = 48000;

    for &fft in sizes.iter() {
        let num_bins = (fft / 2) + 1;
        let spectrum = vec![-40.0; num_bins];
        let mut output = vec![0.0; num_bins];
        calculate_mask_per_bin(&spectrum, fft, sr, &mut output);
    }
}

#[test]
fn mask_deterministic_100_runs() {
    let fft = 1024;
    let num_bins = (fft / 2) + 1;
    let sr = 48000;

    // We use a simple pseudo-random sequence for testing
    let mut spectrum = vec![0.0; num_bins];
    for i in 0..num_bins {
        spectrum[i] = -60.0 + (i % 30) as f32;
    }

    let mut output1 = vec![0.0; num_bins];
    calculate_mask_per_bin(&spectrum, fft, sr, &mut output1);

    for _ in 0..100 {
        let mut output_n = vec![0.0; num_bins];
        calculate_mask_per_bin(&spectrum, fft, sr, &mut output_n);

        for i in 0..num_bins {
            assert_eq!(output1[i], output_n[i], "Determinism failure at bin {}", i);
        }
    }
}
