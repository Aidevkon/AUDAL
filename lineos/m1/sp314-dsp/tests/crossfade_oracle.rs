use sp314_dsp::cut_heal::crossfade::{equal_gain_crossfade, equal_power_crossfade, DEFAULT_XFADE};
use std::f32;

fn rms(slice: &[f32]) -> f32 {
    if slice.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = slice.iter().map(|&x| x * x).sum();
    (sum_sq / slice.len() as f32).sqrt()
}

fn amp_to_db(amp: f32) -> f32 {
    if amp <= 1e-10 {
        -200.0
    } else {
        20.0 * amp.log10()
    }
}

fn generate_noise(samples: usize, seed_init: u32) -> Vec<f32> {
    let mut buf = vec![0.0; samples];
    let mut seed = seed_init;
    for s in buf.iter_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let val = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
        *s = val;
    }
    buf
}

#[test]
fn test_crossfade_null() {
    let len = 1000;
    let fade_len = DEFAULT_XFADE;
    let a = vec![1.0; len];
    let b = vec![1.0; len];

    let out = equal_power_crossfade(&a, &b, fade_len);

    assert_eq!(out.len(), len, "Output length must match input length");

    let mut max_dev = 0.0_f32;
    for i in 0..len {
        assert!(out[i].is_finite(), "Output must be finite");
        let dev = (out[i] - a[i]).abs();
        if dev > max_dev {
            max_dev = dev;
        }
    }

    let bump_db = amp_to_db(1.0 + max_dev) - amp_to_db(1.0);
    println!(
        "CROSSFADE|NULL|max_deviation_amp={:.4}|bump_db={:.4}",
        max_dev, bump_db
    );
}

#[test]
fn test_crossfade_click() {
    let len = 1000;
    let fade_len = DEFAULT_XFADE;
    let a = vec![1.0; len];
    let b = vec![-1.0; len];

    let out = equal_power_crossfade(&a, &b, fade_len);

    let max_allowed_delta = (2.0 / fade_len as f32) * 1.5;

    for i in 1..fade_len {
        let delta = out[i] - out[i - 1];
        assert!(
            delta <= 0.0,
            "Fade must be monotonic. Failed at index {}",
            i
        );
        assert!(
            delta.abs() < max_allowed_delta,
            "Sample delta {} exceeds max allowed {}",
            delta.abs(),
            max_allowed_delta
        );
    }
}

#[test]
fn test_crossfade_determinism() {
    let len = 1000;
    let fade_len = DEFAULT_XFADE;
    let a = generate_noise(len, 42);
    let b = generate_noise(len, 123);

    let out1 = equal_power_crossfade(&a, &b, fade_len);
    let out2 = equal_power_crossfade(&a, &b, fade_len);

    for (s1, s2) in out1.iter().zip(out2.iter()) {
        assert_eq!(s1.to_bits(), s2.to_bits(), "Determinism broken");
    }
}

#[test]
fn test_crossfade_energy() {
    let len = 10000;
    let fade_len = 8000;
    let a = generate_noise(len, 42);
    let b = generate_noise(len, 123);

    let out = equal_power_crossfade(&a, &b, fade_len);

    let rms_a = rms(&a[0..fade_len]);
    let rms_b = rms(&b[0..fade_len]);
    let rms_out = rms(&out[0..fade_len]);

    let db_a = amp_to_db(rms_a);
    let db_b = amp_to_db(rms_b);
    let db_out = amp_to_db(rms_out);

    let target_db = (db_a + db_b) / 2.0;
    let delta_db = db_out - target_db;

    println!(
        "CROSSFADE|ENERGY|rms_a_db={:.4}|rms_b_db={:.4}|rms_out_db={:.4}|delta_db={:.4}",
        db_a, db_b, db_out, delta_db
    );
}

#[test]
fn test_equal_gain_null() {
    let len = 1000;
    let fade_len = DEFAULT_XFADE;
    let a = vec![1.0; len];
    let b = vec![1.0; len];

    let out = equal_gain_crossfade(&a, &b, fade_len);

    assert_eq!(out.len(), len, "Output length must match input length");

    let mut max_dev = 0.0_f32;
    for i in 0..len {
        assert!(out[i].is_finite(), "Output must be finite");
        let dev = (out[i] - a[i]).abs();
        if dev > max_dev {
            max_dev = dev;
        }
    }

    assert!(
        max_dev < 1e-6,
        "Equal-gain should be exact on identical signals. Max dev: {}",
        max_dev
    );
}

#[test]
fn test_equal_gain_click() {
    let len = 1000;
    let fade_len = DEFAULT_XFADE;
    let a = vec![1.0; len];
    let b = vec![-1.0; len];

    let out = equal_gain_crossfade(&a, &b, fade_len);

    let max_allowed_delta = (2.0 / fade_len as f32) * 1.5;

    for i in 1..fade_len {
        let delta = out[i] - out[i - 1];
        assert!(
            delta <= 0.0,
            "Fade must be monotonic. Failed at index {}",
            i
        );
        assert!(
            delta.abs() < max_allowed_delta,
            "Sample delta {} exceeds max allowed {}",
            delta.abs(),
            max_allowed_delta
        );
    }
}

#[test]
fn test_equal_gain_determinism() {
    let len = 1000;
    let fade_len = DEFAULT_XFADE;
    let a = generate_noise(len, 42);
    let b = generate_noise(len, 123);

    let out1 = equal_gain_crossfade(&a, &b, fade_len);
    let out2 = equal_gain_crossfade(&a, &b, fade_len);

    for (s1, s2) in out1.iter().zip(out2.iter()) {
        assert_eq!(s1.to_bits(), s2.to_bits(), "Determinism broken");
    }
}

#[test]
fn test_equal_gain_energy() {
    let len = 10000;
    let fade_len = 8000;
    let a = generate_noise(len, 42);
    let b = generate_noise(len, 123);

    let out = equal_gain_crossfade(&a, &b, fade_len);

    let rms_a = rms(&a[0..fade_len]);
    let rms_b = rms(&b[0..fade_len]);
    let rms_out = rms(&out[0..fade_len]);

    let db_a = amp_to_db(rms_a);
    let db_b = amp_to_db(rms_b);
    let db_out = amp_to_db(rms_out);

    let target_db = (db_a + db_b) / 2.0;
    let delta_db = db_out - target_db;

    println!(
        "CROSSFADE_EQUAL_GAIN|ENERGY|rms_a_db={:.4}|rms_b_db={:.4}|rms_out_db={:.4}|delta_db={:.4}",
        db_a, db_b, db_out, delta_db
    );
}
