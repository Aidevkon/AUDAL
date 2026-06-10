use serde_json::Value;
use std::fs;

fn load_fixture(name: &str) -> Value {
    let path = format!("tests/fixtures/{}.json", name);
    let content = fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read {}", path));
    serde_json::from_str(&content).expect("Failed to parse JSON")
}

#[test]
fn crossover3_sum_is_flat() {
    use sp314_dsp::compressor::crossover::CrossoverLR4x3;

    let fixture = load_fixture("crossover3_reference");
    let f_low = fixture["f_low_hz"].as_f64().unwrap() as f32;
    let f_high = fixture["f_high_hz"].as_f64().unwrap() as f32;
    let fs = fixture["sample_rate"].as_f64().unwrap() as u32;
    let n_settle = fixture["n_settle"].as_u64().unwrap() as usize;
    let n_measure = fixture["n_measure"].as_u64().unwrap() as usize;

    let checks = fixture["sum_checks"].as_array().unwrap();

    for check in checks {
        let freq_hz = check["freq_hz"].as_f64().unwrap() as f32;
        let tol = check["tolerance_db"].as_f64().unwrap() as f32;

        let mut lr4x3 = CrossoverLR4x3::new(f_low, f_high, fs);

        let w = 2.0 * std::f32::consts::PI * freq_hz / fs as f32;

        // Settle
        for i in 0..n_settle {
            let x = (i as f32 * w).sin();
            lr4x3.process(x);
        }

        // Measure peak sum
        let mut peak_sum = 0.0_f32;
        for i in 0..n_measure {
            let x = ((n_settle + i) as f32 * w).sin();
            let (low, mid, high) = lr4x3.process(x);
            let sum = (low + mid + high).abs();
            if sum > peak_sum {
                peak_sum = sum;
            }
        }

        let sum_db = if peak_sum < 1e-12 {
            -144.0_f32
        } else {
            20.0 * peak_sum.log10()
        };

        assert!(
            (sum_db - 0.0_f32).abs() < tol,
            "freq={}Hz: sum={:.3}dB expected 0.0dB ±{}dB",
            freq_hz,
            sum_db,
            tol
        );
    }
}

#[test]
fn crossover3_split_at_crossover_freqs() {
    use sp314_dsp::compressor::crossover::CrossoverLR4x3;

    let fixture = load_fixture("crossover3_reference");
    let f_low = fixture["f_low_hz"].as_f64().unwrap() as f32;
    let f_high = fixture["f_high_hz"].as_f64().unwrap() as f32;
    let fs = fixture["sample_rate"].as_f64().unwrap() as u32;
    let n_settle = fixture["n_settle"].as_u64().unwrap() as usize;
    let n_measure = fixture["n_measure"].as_u64().unwrap() as usize;

    // At f_low: low band should be -6dB
    {
        let mut lr4x3 = CrossoverLR4x3::new(f_low, f_high, fs);
        let w = 2.0 * std::f32::consts::PI * f_low / fs as f32;
        for i in 0..n_settle {
            lr4x3.process((i as f32 * w).sin());
        }
        let mut peak_low = 0.0_f32;
        for i in 0..n_measure {
            let x = ((n_settle + i) as f32 * w).sin();
            let (low, _mid, _high) = lr4x3.process(x);
            if low.abs() > peak_low {
                peak_low = low.abs();
            }
        }
        let low_db = 20.0 * peak_low.log10();
        assert!(
            (low_db - (-6.0)).abs() < 0.2,
            "At f_low={}: low band={:.3}dB expected -6.0dB ±0.2dB",
            f_low,
            low_db
        );
    }

    // At f_high: high band should be -6dB
    {
        let mut lr4x3 = CrossoverLR4x3::new(f_low, f_high, fs);
        let w = 2.0 * std::f32::consts::PI * f_high / fs as f32;
        for i in 0..n_settle {
            lr4x3.process((i as f32 * w).sin());
        }
        let mut peak_high = 0.0_f32;
        for i in 0..n_measure {
            let x = ((n_settle + i) as f32 * w).sin();
            let (_low, _mid, high) = lr4x3.process(x);
            if high.abs() > peak_high {
                peak_high = high.abs();
            }
        }
        let high_db = 20.0 * peak_high.log10();
        assert!(
            (high_db - (-6.0)).abs() < 0.2,
            "At f_high={}: high band={:.3}dB expected -6.0dB ±0.2dB",
            f_high,
            high_db
        );
    }
}

#[test]
fn crossover3_no_sign_inversion() {
    use sp314_dsp::compressor::crossover::CrossoverLR4x3;

    let fixture = load_fixture("crossover3_reference");
    let f_low = fixture["f_low_hz"].as_f64().unwrap() as f32;
    let f_high = fixture["f_high_hz"].as_f64().unwrap() as f32;
    let fs = fixture["sample_rate"].as_f64().unwrap() as u32;

    let mut lr4x3 = CrossoverLR4x3::new(f_low, f_high, fs);

    // Feed impulse — sum of all bands must be positive at sample 0
    let (low, mid, high) = lr4x3.process(1.0);
    let sum = low + mid + high;
    assert!(
        sum > 0.0,
        "Sign inversion detected: impulse sum={:.6} should be positive",
        sum
    );
}
