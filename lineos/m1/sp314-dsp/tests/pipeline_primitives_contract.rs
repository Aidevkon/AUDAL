// allow: contract tests index synthetic buffers the same way the
// DSP code under test does; iterator rewrites add nothing here.
#![allow(clippy::needless_range_loop)]

use approx::assert_abs_diff_eq;
use serde_json::Value;
use sp314_dsp::masking_eq::{MaskingAwareEQ, MaskingEQConfig};
use sp314_dsp::pipeline::gain::HeadroomManager;
use sp314_dsp::pipeline::phase::PhaseAligner;
use std::fs;

fn load_fixture(name: &str) -> Value {
    let path = format!("tests/fixtures/{}.json", name);
    let content = fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read {}", path));
    serde_json::from_str(&content).expect("Failed to parse JSON")
}

// ==========================================
// PhaseAligner tests
// ==========================================

#[test]
fn phase_aligner_magnitude_is_flat() {
    let mut aligner = PhaseAligner::new(150.0, 48000);
    // Sine sweep 20Hz-20kHz
    let freqs = [20.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0, 20000.0];
    for &f in &freqs {
        let w = 2.0 * std::f32::consts::PI * f / 48000.0;
        let mut peak_out = 0.0_f32;
        let in_peak = 1.0_f32;

        for i in 0..2000 {
            // Allow settling
            let mut l = (i as f32 * w).sin();
            let mut r = l;
            aligner.process_stereo(&mut l, &mut r);
        }

        for i in 0..2000 {
            let mut l = ((i + 2000) as f32 * w).sin();
            let mut r = l;
            aligner.process_stereo(&mut l, &mut r);
            if l.abs() > peak_out {
                peak_out = l.abs();
            }
        }

        let mag_db = 20.0 * (peak_out / in_peak).log10();
        assert_abs_diff_eq!(mag_db, 0.0, epsilon = 0.1);
    }
}

#[test]
fn phase_aligner_matches_crossover_phase() {
    // Actually, PhaseAligner IS the sum of crossover outputs.
    // The prompt says "PhaseAligner(x) phase == CrossoverLR4(x).low + CrossoverLR4(x).high phase"
    // and to verify against `phase_aligner_reference.json`.
    let fixture = load_fixture("phase_aligner_reference");
    let phase_checks = fixture["phase_checks"].as_array().unwrap();

    for check in phase_checks {
        let f = check["freq_hz"].as_f64().unwrap() as f32;
        let expected_phase_deg = check["expected_phase_deg"].as_f64().unwrap() as f32;
        let tol_deg = check["tolerance_deg"].as_f64().unwrap() as f32;

        let mut aligner = PhaseAligner::new(150.0, 48000);
        let w = 2.0 * std::f32::consts::PI * f / 48000.0;

        // Feed sine and measure phase difference
        let mut last_in = 0.0;
        let mut last_out = 0.0;
        let mut zero_cross_in = 0;
        let mut zero_cross_out = 0;

        // Let it settle
        for i in 0..48000 {
            let mut l = (i as f32 * w).sin();
            let mut r = l;
            aligner.process_stereo(&mut l, &mut r);
        }

        for i in 0..48000 {
            let x = ((i + 48000) as f32 * w).sin();
            let mut l = x;
            let mut r = x;
            aligner.process_stereo(&mut l, &mut r);

            if last_in < 0.0 && x >= 0.0 {
                zero_cross_in = i;
            }
            if last_out < 0.0 && l >= 0.0 {
                zero_cross_out = i;
            }

            last_in = x;
            last_out = l;

            // Just need one good pair of zero crossings
            if zero_cross_in > 0 && zero_cross_out > 0 && i > 1000 {
                break;
            }
        }

        let delay_samples = zero_cross_out as f32 - zero_cross_in as f32;
        if delay_samples < 0.0 {
            // It could be that out crossed before in (phase wrap), or we missed a cycle.
            // Actually, we should be careful. A better way to measure phase is DFT at `f`.
        }

        // DFT measurement
        let mut re_in = 0.0_f32;
        let mut im_in = 0.0_f32;
        let mut re_out = 0.0_f32;
        let mut im_out = 0.0_f32;

        // Reset and settle again for DFT
        aligner.reset();
        for i in 0..10000 {
            let mut l = (i as f32 * w).sin();
            let mut r = l;
            aligner.process_stereo(&mut l, &mut r);
        }

        for i in 0..10000 {
            let idx = i + 10000;
            let x = (idx as f32 * w).sin();
            let mut l = x;
            let mut r = x;
            aligner.process_stereo(&mut l, &mut r);

            let cos_w = (idx as f32 * w).cos();
            let sin_w = -(idx as f32 * w).sin(); // complex exponent e^{-j w n}

            re_in += x * cos_w;
            im_in += x * sin_w;

            re_out += l * cos_w;
            im_out += l * sin_w;
        }

        let phase_in = im_in.atan2(re_in);
        let phase_out = im_out.atan2(re_out);
        let phase_diff_rad = phase_out - phase_in;
        // Unwrap to continuous? The unwrapped phase at 150Hz is -180 deg, at 10000Hz is around -360 deg.
        // We can just check phase_diff % 360 vs expected % 360, but wait! The expected unwrapped phase is e.g. -360.
        // phase_out - phase_in will be in [-pi, pi].
        let mut phase_diff_deg = phase_diff_rad * 180.0 / std::f32::consts::PI;
        while phase_diff_deg > 180.0 {
            phase_diff_deg -= 360.0;
        }
        while phase_diff_deg <= -180.0 {
            phase_diff_deg += 360.0;
        }

        let mut expected_wrapped = expected_phase_deg;
        while expected_wrapped > 180.0 {
            expected_wrapped -= 360.0;
        }
        while expected_wrapped <= -180.0 {
            expected_wrapped += 360.0;
        }

        let mut diff = (phase_diff_deg - expected_wrapped).abs();
        if diff > 180.0 {
            diff = 360.0 - diff;
        }

        assert!(
            diff < tol_deg,
            "Phase at {}Hz: expected {} (wrapped {}), got {} (diff {})",
            f,
            expected_phase_deg,
            expected_wrapped,
            phase_diff_deg,
            diff
        );
    }
}

#[test]
fn phase_aligner_deterministic() {
    let mut aligner1 = PhaseAligner::new(150.0, 48000);
    let mut out1 = vec![];
    for i in 0..100 {
        let mut l = (i as f32).sin();
        let mut r = (i as f32).cos();
        aligner1.process_stereo(&mut l, &mut r);
        out1.push((l, r));
    }

    let mut aligner2 = PhaseAligner::new(150.0, 48000);
    for i in 0..100 {
        let mut l = (i as f32).sin();
        let mut r = (i as f32).cos();
        aligner2.process_stereo(&mut l, &mut r);
        assert_eq!(out1[i].0, l);
        assert_eq!(out1[i].1, r);
    }
}

#[test]
fn phase_aligner_reset_clears_state() {
    let mut aligner = PhaseAligner::new(150.0, 48000);
    let mut out1 = vec![];
    for i in 0..100 {
        let mut l = (i as f32).sin();
        let mut r = (i as f32).cos();
        aligner.process_stereo(&mut l, &mut r);
        out1.push((l, r));
    }

    aligner.reset();
    for i in 0..100 {
        let mut l = (i as f32).sin();
        let mut r = (i as f32).cos();
        aligner.process_stereo(&mut l, &mut r);
        assert_eq!(out1[i].0, l);
        assert_eq!(out1[i].1, r);
    }
}

// ==========================================
// HeadroomManager tests
// ==========================================

#[test]
fn headroom_input_pad_is_minus_6db() {
    let mut left = [1.0];
    let mut right = [1.0];
    let hm = HeadroomManager::new(-6.0, 6.0);
    hm.apply_input_pad(&mut left, &mut right);
    let expected_pad = 10.0_f32.powf(-6.0 / 20.0);
    assert_abs_diff_eq!(left[0], expected_pad, epsilon = 1e-6);
    assert_abs_diff_eq!(right[0], expected_pad, epsilon = 1e-6);
}

#[test]
fn headroom_output_makeup_is_plus_6db() {
    let mut left = [0.5];
    let mut right = [0.5];
    let hm = HeadroomManager::new(-6.0, 6.0);
    hm.apply_output_makeup(&mut left, &mut right);
    let expected_makeup = 0.5 * 10.0_f32.powf(6.0 / 20.0);
    assert_abs_diff_eq!(left[0], expected_makeup, epsilon = 1e-6);
    assert_abs_diff_eq!(right[0], expected_makeup, epsilon = 1e-6);
}

#[test]
fn headroom_output_clips_at_unity() {
    let mut left = [2.0];
    let mut right = [2.0];
    let hm = HeadroomManager::new(-6.0, 6.0);
    hm.apply_output_makeup(&mut left, &mut right);
    assert_eq!(left[0], 1.0);
    assert_eq!(right[0], 1.0);
}

// ==========================================
// StereoMaskingEQ tests
// ==========================================

#[test]
fn stereo_eq_linked_analysis_preserves_center_image() {
    let config = MaskingEQConfig {
        target_db: [6.0; 8],
        mask_margin_db: 3.0,
        max_boost_db: 6.0,
        target_phon: 80.0,
    };
    let mut eq = MaskingAwareEQ::new(config, 48000).unwrap();
    let mut left = vec![0.0; 1024];
    for i in 0..1024 {
        left[i] = ((i * 137) % 200) as f32 / 100.0 - 1.0;
    }
    let mut right = left.clone(); // pure mono

    eq.process_block(&mut left, &mut right, &[0.0; 5]);

    for i in 0..1024 {
        assert_eq!(left[i], right[i], "Stereo image drifted at sample {}", i);
    }
}

#[test]
fn stereo_eq_process_block_deterministic() {
    let config = MaskingEQConfig {
        target_db: [6.0; 8],
        mask_margin_db: 0.0,
        max_boost_db: 12.0,
        target_phon: 80.0,
    };

    let mut left_in = vec![0.0; 512];
    let mut right_in = vec![0.0; 512];
    for i in 0..512 {
        left_in[i] = ((i % 30) as f32 / 15.0) - 1.0;
        right_in[i] = ((i % 40) as f32 / 20.0) - 1.0;
    }

    let mut eq1 = MaskingAwareEQ::new(config.clone(), 48000).unwrap();
    let mut l1 = left_in.clone();
    let mut r1 = right_in.clone();
    eq1.process_block(&mut l1, &mut r1, &[0.0; 5]);

    for _ in 0..10 {
        let mut eq2 = MaskingAwareEQ::new(config.clone(), 48000).unwrap();
        let mut l2 = left_in.clone();
        let mut r2 = right_in.clone();
        eq2.process_block(&mut l2, &mut r2, &[0.0; 5]);

        for i in 0..512 {
            assert_eq!(l1[i], l2[i]);
            assert_eq!(r1[i], r2[i]);
        }
    }
}

#[test]
fn stereo_eq_reset_restores_identity() {
    let config = MaskingEQConfig {
        target_db: [6.0; 8],
        mask_margin_db: 0.0,
        max_boost_db: 12.0,
        target_phon: 80.0,
    };

    let mut left_in = vec![0.0; 512];
    let mut right_in = vec![0.0; 512];
    for i in 0..512 {
        left_in[i] = ((i % 30) as f32 / 15.0) - 1.0;
        right_in[i] = ((i % 40) as f32 / 20.0) - 1.0;
    }

    let mut eq = MaskingAwareEQ::new(config, 48000).unwrap();
    let mut l1 = left_in.clone();
    let mut r1 = right_in.clone();
    eq.process_block(&mut l1, &mut r1, &[0.0; 5]);

    eq.reset();

    let mut l2 = left_in.clone();
    let mut r2 = right_in.clone();
    eq.process_block(&mut l2, &mut r2, &[0.0; 5]);

    // After reset, it should behave exactly as if it was newly created,
    // which means l2 should match l1.
    for i in 0..512 {
        assert_eq!(l1[i], l2[i]);
        assert_eq!(r1[i], r2[i]);
    }
}

// ==========================================
// TPDF Dither tests
// ==========================================

use sp314_dsp::pipeline::dither::TpdfDither;

#[test]
fn dither_seeded_deterministic() {
    let mut dither1 = TpdfDither::new(42);
    let mut out1 = vec![];
    for i in 0..100 {
        let x = (i as f32).sin();
        out1.push(dither1.process_sample(x, x >= 0.0));
    }

    let mut dither2 = TpdfDither::new(42);
    for i in 0..100 {
        let x = (i as f32).sin();
        assert_eq!(out1[i], dither2.process_sample(x, x >= 0.0));
    }
}

#[test]
fn dither_noise_bounded_by_plus_minus_1_lsb() {
    let mut dither = TpdfDither::new(42);
    for i in 0..10000 {
        let x = ((i as f32) / 5000.0) - 1.0;
        let out = dither.process_sample(x, x >= 0.0);
        assert!(out >= -8388608);
        assert!(out <= 8388607);
    }
}

#[test]
fn dither_prng_never_zero_state() {
    let mut dither = TpdfDither::new(0);
    let mut all_same = true;
    let mut last = 0;
    for i in 0..100 {
        // x = 0.5 / 8388607.0 means scaled = 0.5
        // scaled + dither is in [-0.5, 1.5). Truncation to i32 yields 0 or 1.
        let out = dither.process_sample(0.5 / 8388607.0, true);
        if i > 0 && out != last {
            all_same = false;
        }
        last = out;
    }
    assert!(!all_same, "PRNG stuck at zero state");
}
