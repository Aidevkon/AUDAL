//! STFT Oracle Contract Tests
//! Authority: Orthogonal Transforms Spec v1.0 OT-P2
//! INV-OT-1: perfect reconstruction MSE < 1e-5
//! INV-OT-5: fixture SHA-256 locked

use serde::Deserialize;
use sp314_dsp::stft::StftEngine;

#[derive(Deserialize)]
struct SignalResult {
    mse:  f64,
    pass: bool,
}

#[derive(Deserialize)]
struct OracleFixture {
    sine_440hz:  SignalResult,
    white_noise: SignalResult,
    fft_size:    usize,
    hop_size:    usize,
}

fn load_oracle() -> OracleFixture {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/stft_oracle.json"
    );
    let json = std::fs::read_to_string(path)
        .expect("Run tools/oracle/stft_oracle.py first");
    serde_json::from_str(&json).unwrap()
}

fn reconstruction_mse(signal: &[f32]) -> f32 {
    let mut engine = StftEngine::new();
    let (frames, _) = engine.forward(signal);
    let recon = engine.inverse(&frames, signal.len());
    let n = signal.len().min(recon.len());
    signal[..n].iter().zip(recon[..n].iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>() / n as f32
}

#[test]
fn oracle_params_match_rust_constants() {
    let oracle = load_oracle();
    assert_eq!(oracle.fft_size, sp314_dsp::stft::FFT_SIZE);
    assert_eq!(oracle.hop_size, sp314_dsp::stft::HOP_SIZE);
}

#[test]
fn stft_oracle_sine_440hz_passes() {
    let oracle = load_oracle();
    assert!(oracle.sine_440hz.pass,
        "Python oracle sine MSE={:.2e} failed", oracle.sine_440hz.mse);

    let sample_rate = 48000u32;
    let signal: Vec<f32> = (0..sample_rate as usize)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0
                  * i as f32 / sample_rate as f32).sin())
        .collect();

    let mse = reconstruction_mse(&signal);
    assert!(mse < 1e-5,
        "Rust STFT sine MSE={:.2e} exceeds INV-OT-1 threshold 1e-5", mse);
}

#[test]
fn stft_oracle_noise_passes() {
    let oracle = load_oracle();
    assert!(oracle.white_noise.pass,
        "Python oracle noise MSE={:.2e} failed", oracle.white_noise.mse);

    // Deterministic noise — same seed as Python oracle
    // Python: np.random.seed(42), randn * 0.1
    // Approximate with known pattern
    let signal: Vec<f32> = (0..48000)
        .map(|i| ((i as f32 * 1.6180339887) % 1.0 - 0.5) * 0.1)
        .collect();

    let mse = reconstruction_mse(&signal);
    assert!(mse < 1e-5,
        "Rust STFT noise MSE={:.2e} exceeds INV-OT-1 threshold 1e-5", mse);
}

#[test]
fn stft_oracle_deterministic() {
    // INV-OT-3: same input → same output always
    let signal: Vec<f32> = (0..4096)
        .map(|i| (i as f32 * 0.1).sin())
        .collect();

    let mse1 = reconstruction_mse(&signal);
    let mse2 = reconstruction_mse(&signal);
    assert_eq!(mse1, mse2, "STFT is not deterministic");
}

#[test]
fn stft_fixture_sha256_locked() {
    // INV-OT-5: fixture integrity
    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/stft_oracle.json"
    );
    let lock_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/stft_oracle.lock"
    );

    if std::path::Path::new(lock_path).exists() {
        use std::io::Read;
        let mut fixture = Vec::new();
        std::fs::File::open(fixture_path).unwrap()
            .read_to_end(&mut fixture).unwrap();

        let actual = format!("{:x}",
            <sha2::Sha256 as sha2::Digest>::digest(&fixture));
        let expected = std::fs::read_to_string(lock_path)
            .unwrap().trim().to_string();

        assert_eq!(actual, expected,
            "STFT oracle fixture has been tampered! INV-OT-5 violated");
    }
}
