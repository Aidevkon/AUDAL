use sp314_dsp::analysis::onset_flux::SuperFluxOnset;
use sp314_dsp::analysis::superflux_weights::MEL_BANDS;
use sp314_dsp::stft::{StftEngine, HOP_SIZE};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

fn read_f64_bin(path: &str) -> Vec<f64> {
    let mut file = File::open(path).expect(&format!("Could not open {}", path));
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    let mut f64s = Vec::with_capacity(buf.len() / 8);
    for chunk in buf.chunks_exact(8) {
        f64s.push(f64::from_le_bytes(chunk.try_into().unwrap()));
    }
    f64s
}

fn load_wav_mono(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let mut samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    };
    
    if channels > 1 {
        let mut mono = Vec::with_capacity(samples.len() / channels);
        for chunk in samples.chunks(channels) {
            let mut sum = 0.0;
            for s in chunk { sum += s; }
            mono.push(sum / channels as f32);
        }
        mono
    } else {
        samples
    }
}

#[test]
fn test_superflux_oracle() {
    let fixtures = vec![
        "bodleasons_mid",
        "clip_speech",
        "clip_podcast_st",
    ];

    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../research/superflux/outputs");
    let wav_base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures");
    let flight_base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../flight_clips");
    let flight_st_base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../flight_clips_stereo");

    for fix in fixtures {
        println!("SFLUX| Fixture: {}", fix);

        let matrix_path = base.join(format!("{}_matrix.bin", fix));
        let env_path = base.join(format!("{}_envelope.bin", fix));

        let oracle_matrix = read_f64_bin(matrix_path.to_str().unwrap());
        let oracle_env = read_f64_bin(env_path.to_str().unwrap());

        let num_frames = oracle_env.len();
        
        let wav_path = if fix == "bodleasons_mid" {
            wav_base.join("bodleasons_mid.wav")
        } else if fix == "clip_speech" {
            flight_base.join("clip_speech.wav")
        } else {
            flight_st_base.join("clip_podcast_st.wav")
        };
        let mono_audio = load_wav_mono(wav_path.to_str().unwrap());
        
        let mut stft = StftEngine::new();
        let (frames, _) = stft.forward(&mono_audio);
        
        let mut sf_a = SuperFluxOnset::new(10.0);
        let mut sf_b = SuperFluxOnset::new(10.0);
        
        let mut gate_a_diff = 0.0;
        let mut max_gate_a = 0.0;
        let mut gate_b_diff = 0.0;
        let mut max_gate_b = 0.0;
        
        let test_frames = frames.len().min(num_frames);
        
        for t in 0..test_frames {
            // Gate A: E2E from Audio
            let mut stft_mag = [0.0_f32; 1025];
            for k in 0..1025 {
                stft_mag[k] = frames[t][k].norm();
            }
            let s_log_a = sf_a.compute_log_bands(&stft_mag);
            let rust_env_a = sf_a.process_log_bands(&s_log_a);
            
            // Gate B: Math only from Python bands
            let mut s_log_b = [0.0_f32; MEL_BANDS];
            for b in 0..MEL_BANDS {
                s_log_b[b] = oracle_matrix[b * num_frames + t] as f32; // librosa exports [bands, frames]
            }
            let rust_env_b = sf_b.process_log_bands(&s_log_b);
            
            // Librosa pads the onset envelope with `lag + n_fft // (2*hop) = 3` zeros at the start, 
            // discarding S[0] - ref[-1].
            // Thus, rust_env_b[t] == python_env[t + 2] for t >= 1.
            let python_env = if t >= 1 && (t + 2) < num_frames {
                oracle_env[t + 2] as f32
            } else {
                0.0 // Don't compare at boundaries where python zero-pads
            };
            
            if t >= 1 && (t + 2) < num_frames {
                let diff_a = (rust_env_a - python_env).abs();
                gate_a_diff += diff_a as f64;
                if diff_a > max_gate_a { max_gate_a = diff_a; }
                
                let diff_b = (rust_env_b - python_env).abs();
                gate_b_diff += diff_b as f64;
                if diff_b > max_gate_b { max_gate_b = diff_b; }
            }
            
            // Print landscape sample every ~2 seconds (187 frames)
            if t % 187 == 0 {
                println!("SFLUX|   t={:0.2}s  env_A={:.4}  env_B={:.4}  librosa={:.4}", t as f32 * HOP_SIZE as f32 / 48000.0, rust_env_a, rust_env_b, python_env);
            }
        }
        
        let valid_frames = test_frames.saturating_sub(3).max(1);
        println!("SFLUX| {} Gate A (E2E STFT vs librosa) - Avg Diff: {:.6}, Max Diff: {:.6}", fix, gate_a_diff / valid_frames as f64, max_gate_a);
        println!("SFLUX| {} Gate B (Math only env vs librosa) - Avg Diff: {:.6}, Max Diff: {:.6}", fix, gate_b_diff / valid_frames as f64, max_gate_b);
        
        assert!(max_gate_b < 1e-4, "Gate B math deviation too high!");
    }
}
