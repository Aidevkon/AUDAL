use std::fs;
use std::process::Command;
use tempfile::TempDir;

// Test-only writer — production never writes WAV here.
fn write_wav(path: &std::path::Path, samples: &[i16]) {
    use std::io::Write;
    let mut file = std::fs::File::create(path).unwrap();
    let byte_rate = 48000 * 2 * 2;
    let data_size = samples.len() * 2;
    let total_size = 36 + data_size;

    file.write_all(b"RIFF").unwrap();
    file.write_all(&(total_size as u32).to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();
    file.write_all(b"fmt ").unwrap();
    file.write_all(&(16u32).to_le_bytes()).unwrap();
    file.write_all(&(1u16).to_le_bytes()).unwrap(); // PCM
    file.write_all(&(2u16).to_le_bytes()).unwrap(); // Stereo
    file.write_all(&(48000u32).to_le_bytes()).unwrap();
    file.write_all(&(byte_rate as u32).to_le_bytes()).unwrap();
    file.write_all(&(4u16).to_le_bytes()).unwrap(); // block align
    file.write_all(&(16u16).to_le_bytes()).unwrap(); // bits per sample
    file.write_all(b"data").unwrap();
    file.write_all(&(data_size as u32).to_le_bytes()).unwrap();

    let mut data_bytes = Vec::with_capacity(data_size);
    for &s in samples {
        data_bytes.extend_from_slice(&s.to_le_bytes());
    }
    file.write_all(&data_bytes).unwrap();
}

// Known: Butterworth skirt leakage makes band 0 read the 141.4 carrier ~12dB down
// and compresses the band1→2 step ~3.7dB — expected, not a bug.
// Known: lra differs ~0.002 LU between buckets (16-bit
// quantization after different peak norms) — file-vs-file effect,
// irrelevant to the run-vs-run byte-compare this test performs.
fn generate_fixture(amps: &[f64; 6], peak_norm: f64) -> Vec<i16> {
    let sr = 48000;
    let duration = 6;
    let total_samples = sr * duration;
    let freqs = [141.4, 353.6, 707.1, 1414.2, 2828.4, 5656.9];

    let mut float_samples = Vec::with_capacity(total_samples);
    let mut max_val = 0.0;

    for i in 0..total_samples {
        let t = i as f64 / sr as f64;
        let mut val = 0.0;
        for j in 0..6 {
            val += amps[j] * f64::sin(2.0 * std::f64::consts::PI * freqs[j] * t);
        }

        let interval = i / 24000;
        let pos = i % 24000;
        let target_env = if interval % 2 == 0 { 1.0 } else { 0.25 };
        let prev_env = if interval % 2 == 0 { 0.25 } else { 1.0 };

        let env = if pos < 480 {
            let x = pos as f64 / 480.0;
            let a = if i < 480 { 0.0 } else { prev_env };
            let b = target_env;
            a + (b - a) * 0.5 * (1.0 - f64::cos(std::f64::consts::PI * x))
        } else {
            target_env
        };

        val *= env;
        float_samples.push(val);
        if val.abs() > max_val {
            max_val = val.abs();
        }
    }

    let scale = if max_val > 0.0 {
        peak_norm / max_val
    } else {
        1.0
    };

    let mut pcm = Vec::with_capacity(total_samples * 2);
    for v in float_samples {
        let mut int_val = (32767.0 * v * scale) as i32;
        if int_val > 32767 {
            int_val = 32767;
        }
        if int_val < -32768 {
            int_val = -32768;
        }
        let s16 = int_val as i16;
        pcm.push(s16); // Left
        pcm.push(s16); // Right
    }

    pcm
}

#[test]
fn test_measure_corpus_determinism() {
    let corpus_dir = TempDir::new().unwrap();
    let out_dir_1 = TempDir::new().unwrap();
    let out_dir_2 = TempDir::new().unwrap();

    let idm_dir = corpus_dir.path().join("idm");
    let acoustic_dir = corpus_dir.path().join("acoustic");
    fs::create_dir_all(&idm_dir).unwrap();
    fs::create_dir_all(&acoustic_dir).unwrap();

    let idm_amps = [0.30, 0.19, 0.12, 0.075, 0.047, 0.030];
    let acoustic_amps = [0.30, 0.126, 0.053, 0.0223, 0.0094, 0.0039];

    let idm_pcm = generate_fixture(&idm_amps, 0.85);
    let acoustic_pcm = generate_fixture(&acoustic_amps, 0.30);

    write_wav(&idm_dir.join("test01.wav"), &idm_pcm);
    write_wav(&idm_dir.join("test02.wav"), &idm_pcm);

    write_wav(&acoustic_dir.join("test01.wav"), &acoustic_pcm);
    write_wav(&acoustic_dir.join("test02.wav"), &acoustic_pcm);

    let bin_path = env!("CARGO_BIN_EXE_measure_corpus");

    // Run 1
    let output1 = Command::new(bin_path)
        .arg(corpus_dir.path())
        .arg(out_dir_1.path())
        .arg("--corpus-version")
        .arg("determinism")
        .output()
        .expect("Failed to execute run 1");

    if !output1.status.success() {
        panic!(
            "Run 1 failed!\nStderr: {}",
            String::from_utf8_lossy(&output1.stderr)
        );
    }

    // Run 2
    let output2 = Command::new(bin_path)
        .arg(corpus_dir.path())
        .arg(out_dir_2.path())
        .arg("--corpus-version")
        .arg("determinism")
        .output()
        .expect("Failed to execute run 2");

    if !output2.status.success() {
        panic!(
            "Run 2 failed!\nStderr: {}",
            String::from_utf8_lossy(&output2.stderr)
        );
    }

    // Compare files
    let expected_files = [
        "corpus-manifest.json",
        "music-idm-v1.json",
        "music-acoustic-v1.json",
        "genre_centroids_generated.rs",
    ];

    for file in &expected_files {
        let p1 = out_dir_1.path().join(file);
        let p2 = out_dir_2.path().join(file);

        assert!(p1.exists(), "File {} missing from run 1", file);
        assert!(p2.exists(), "File {} missing from run 2", file);

        let c1 = fs::read(&p1).unwrap();
        let c2 = fs::read(&p2).unwrap();

        assert_eq!(c1, c2, "File {} is not byte-identical between runs!", file);
    }
}
