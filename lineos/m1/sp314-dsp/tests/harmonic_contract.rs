// tests/harmonic_contract.rs

const PAD_LINEAR: f32 = 0.5011872336272722;
const GLOBAL_K_HARMONIC: f32 = 1.99526166;
const PER_DRIVE: &[(f32, f32)] = &[
    (0.1,  1.99526184),
    (0.5,  1.99526221),
    (1.0,  1.99526242),
    (2.0,  1.99526212),
    (5.0,  1.99526364),
    (10.0, 1.99526302),
];

#[test]
fn harmonic_k_compensation_matches_reference() {
    assert!(GLOBAL_K_HARMONIC >= 0.5 && GLOBAL_K_HARMONIC <= 5.0, "Global K is out of bounds");

    let sample_rate = 48000;
    let num_samples = sample_rate; // 1 second
    
    for &(drive, k_harmonic) in PER_DRIVE.iter() {
        // Generate 1kHz sine at amplitude=1.0, 48000Hz, 1 second
        let mut x = vec![0.0_f32; num_samples];
        for i in 0..num_samples {
            x[i] = (2.0 * std::f32::consts::PI * 1000.0 * (i as f32) / (sample_rate as f32)).sin();
        }

        // Generate padded sine
        let mut x_padded = vec![0.0_f32; num_samples];
        for i in 0..num_samples {
            x_padded[i] = x[i] * PAD_LINEAR;
        }

        // Apply reference waveshaper: y_ref[i] = tanh(drive * x[i])
        let mut y_ref = vec![0.0_f32; num_samples];
        for i in 0..num_samples {
            y_ref[i] = (drive * x[i]).tanh();
        }

        // Apply compensated waveshaper: y_comp[i] = tanh(K_harmonic * drive * x_padded[i])
        let mut y_comp = vec![0.0_f32; num_samples];
        for i in 0..num_samples {
            y_comp[i] = (k_harmonic * drive * x_padded[i]).tanh();
        }

        // Peak-Normalize BOTH signals
        let mut max_ref = 0.0_f32;
        let mut max_comp = 0.0_f32;
        for i in 0..num_samples {
            if y_ref[i].abs() > max_ref { max_ref = y_ref[i].abs(); }
            if y_comp[i].abs() > max_comp { max_comp = y_comp[i].abs(); }
        }

        let mut y_ref_norm = vec![0.0_f32; num_samples];
        let mut y_comp_norm = vec![0.0_f32; num_samples];
        for i in 0..num_samples {
            y_ref_norm[i] = y_ref[i] / max_ref;
            y_comp_norm[i] = y_comp[i] / max_comp;
        }

        // Compute Time-Domain MSE
        let mut mse_sum = 0.0_f32;
        for i in 0..num_samples {
            let diff = y_ref_norm[i] - y_comp_norm[i];
            mse_sum += diff * diff;
        }
        let mse = mse_sum / (num_samples as f32);

        // Assert MSE < 1e-3
        assert!(mse < 1e-3, "MSE {} is too high for drive {}", mse, drive);
    }
}
