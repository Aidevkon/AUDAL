use claxon::FlacReader;
use rubato::{FastFixedIn, PolynomialDegree, Resampler};
use sp314_dsp::stft::nmfd::nmfd_f32;
use sp314_dsp::stft::{StreamingStftEncoder, N_BINS};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

const SR_IN: u32 = 16_000;
const SR_OUT: u32 = 48_000;
const EPS: f32 = 1e-10;

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0)
}

fn create_mel_filterbank(sr: f32, n_fft: usize, n_mels: usize) -> Vec<Vec<f32>> {
    let n_bins = n_fft / 2 + 1;
    let min_mel = hz_to_mel(0.0);
    let max_mel = hz_to_mel(sr / 2.0);
    let mel_points: Vec<f32> = (0..(n_mels + 2))
        .map(|i| min_mel + i as f32 * (max_mel - min_mel) / (n_mels + 1) as f32)
        .collect();
    let hz_points: Vec<f32> = mel_points.into_iter().map(mel_to_hz).collect();

    let bin_freqs: Vec<f32> = (0..n_bins).map(|i| i as f32 * sr / n_fft as f32).collect();

    let mut fbank = vec![vec![0.0f32; n_bins]; n_mels];
    for i in 0..n_mels {
        let f_m_minus = hz_points[i];
        let f_m = hz_points[i + 1];
        let f_m_plus = hz_points[i + 2];
        for b in 0..n_bins {
            let freq = bin_freqs[b];
            if freq >= f_m_minus && freq <= f_m {
                fbank[i][b] = (freq - f_m_minus) / (f_m - f_m_minus);
            } else if freq >= f_m && freq <= f_m_plus {
                fbank[i][b] = (f_m_plus - freq) / (f_m_plus - f_m);
            }
        }
    }
    fbank
}

fn resample(input: &[f32]) -> Vec<f32> {
    let mut resampler = FastFixedIn::<f32>::new(
        SR_OUT as f64 / SR_IN as f64,
        1.0,
        PolynomialDegree::Septic,
        1024,
        1,
    )
    .unwrap();

    let mut output = Vec::new();
    let mut idx = 0;
    while idx < input.len() {
        let chunk_size = std::cmp::min(1024, input.len() - idx);
        let mut chunk = vec![0.0f32; 1024];
        chunk[..chunk_size].copy_from_slice(&input[idx..idx + chunk_size]);

        let out = resampler.process(&[chunk], None).unwrap();
        output.extend_from_slice(&out[0]);
        idx += chunk_size;
    }
    output
}

#[test]
fn test_w_speech_fit() {
    let corpus_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("research/w-speech/corpus");

    let mut all_samples = Vec::new();
    let mut files = std::fs::read_dir(&corpus_dir)
        .unwrap()
        .map(|res| res.unwrap().path())
        .filter(|p| p.extension().unwrap_or_default() == "flac")
        .collect::<Vec<_>>();
    files.sort();

    for file in &files {
        let mut reader = FlacReader::open(&file).unwrap();
        let samples: Vec<f32> = reader
            .samples()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect();
        all_samples.extend(samples);
    }

    let resampled = resample(&all_samples);

    // Weak label logic: windows > floor+20dB
    let window_size = (0.5 * SR_OUT as f32) as usize;
    let mut rms_vals = Vec::new();
    for chunk in resampled.chunks(window_size) {
        let sum_sq: f32 = chunk.iter().map(|&x| x * x).sum();
        rms_vals.push((sum_sq / chunk.len() as f32).sqrt().max(1e-10));
    }

    let floor_rms = rms_vals.iter().copied().fold(f32::INFINITY, f32::min);
    let threshold_rms = floor_rms * 10.0; // +20dB is x10 in amplitude

    let mut speech_samples = Vec::new();
    for (i, chunk) in resampled.chunks(window_size).enumerate() {
        if rms_vals[i] > threshold_rms {
            speech_samples.extend_from_slice(chunk);
        }
    }

    // Convert to STFT magnitude frames
    let mut ctx = StreamingStftEncoder::new();
    let mut frames_cplx = ctx.feed_chunk(&speech_samples);
    frames_cplx.extend(ctx.finish());

    let num_frames = frames_cplx.len();
    let mut magnitude_frames = vec![0.0f32; N_BINS * num_frames];
    for (f, frame) in frames_cplx.iter().enumerate() {
        for (b, c) in frame.iter().enumerate() {
            magnitude_frames[b * num_frames + f] = (c.re * c.re + c.im * c.im).sqrt();
        }
    }

    let n_mels = 128;
    let fbank = create_mel_filterbank(SR_OUT as f32, (N_BINS - 1) * 2, n_mels);
    let mut mel_v = vec![0.0f32; n_mels * num_frames];
    for m in 0..n_mels {
        for f in 0..num_frames {
            let mut sum = 0.0;
            for b in 0..N_BINS {
                sum += fbank[m][b] * magnitude_frames[b * num_frames + f];
            }
            mel_v[m * num_frames + f] = sum;
        }
    }

    let k = 4;
    let tau = 8;
    let num_iter = 12;

    let mut init_w = vec![0.0f32; n_mels * k * tau];
    let mut init_h = vec![0.0f32; k * num_frames];

    let mut lcg_state: u32 = 314159;
    let mut next_rand = || -> f32 {
        lcg_state = lcg_state.wrapping_mul(1664525).wrapping_add(1013904223);
        (lcg_state as f32) / (u32::MAX as f32)
    };

    for x in init_w.iter_mut() {
        *x = next_rand();
    }
    for x in init_h.iter_mut() {
        *x = next_rand();
    }

    let (tensor_w, final_h, cost_speech) = nmfd_f32(
        &mel_v, &init_w, &init_h, n_mels, k, num_frames, tau, num_iter,
    );

    // Save artifact
    let out_dir = corpus_dir.parent().unwrap();
    let bin_path = out_dir.join("w_speech_v1.bin");
    let mut f_bin = File::create(&bin_path).unwrap();
    // f32 LE
    for &val in &tensor_w {
        f_bin.write_all(&val.to_le_bytes()).unwrap();
    }

    // Generate hashes
    let hash_w = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(std::fs::read(&bin_path).unwrap());
        let res = hasher.finalize();
        let mut s = String::with_capacity(64);
        for byte in res {
            s.push_str(&format!("{:02x}", byte));
        }
        s
    };

    let hashes_obj = serde_json::Value::Object({
        let mut map = serde_json::Map::new();
        for file in &files {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(std::fs::read(file).unwrap());
            let res = hasher.finalize();
            let mut s = String::with_capacity(64);
            for byte in res {
                s.push_str(&format!("{:02x}", byte));
            }
            map.insert(
                file.file_name().unwrap().to_string_lossy().into_owned(),
                serde_json::Value::String(s),
            );
        }
        map
    });

    let params = serde_json::json!({
        "dims": {"bins": n_mels, "k": k, "tau": tau},
        "dtype": "f32 LE",
        "protocol": "fit_protocol_v1",
        "iters": num_iter,
        "seed": 314159,
        "weak_label": {"threshold": "floor+20dB", "min_window": "0.5s"},
        "stft": {"n_fft": 2048, "hop_length": 512, "sr": 48000},
        "mel_bands": "128 triangle filters, 0-24kHz",
        "resample": "16k->48k via rubato::FastFixedIn",
        "corpus_source": "http://www.openslr.org/resources/12/dev-clean.tar.gz",
        "attribution": "LibriSpeech (Panayotov et al.), CC BY 4.0",
        "hashes": {"w_speech_v1.bin": hash_w},
        "corpus_manifest": hashes_obj
    });
    let params_path = out_dir.join("w_speech_v1_params.json");
    File::create(&params_path)
        .unwrap()
        .write_all(params.to_string().as_bytes())
        .unwrap();

    // Sanity landscape on music
    let music_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bodleasons_mid.wav");
    let mut reader_m = hound::WavReader::open(music_path).unwrap();
    let samples_m: Vec<f32> = reader_m
        .samples::<i32>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect();
    let mut mono_m = Vec::with_capacity(samples_m.len() / 2);
    for chunk in samples_m.chunks_exact(2) {
        mono_m.push((chunk[0] + chunk[1]) * 0.5);
    }

    let mut ctx_m = StreamingStftEncoder::new();
    let mut frames_m = ctx_m.feed_chunk(&mono_m);
    frames_m.extend(ctx_m.finish());
    let mut mag_m = vec![0.0f32; N_BINS * frames_m.len()];
    for (f, frame) in frames_m.iter().enumerate() {
        for (b, c) in frame.iter().enumerate() {
            mag_m[b * frames_m.len() + f] = (c.re * c.re + c.im * c.im).sqrt();
        }
    }
    let mut mel_m = vec![0.0f32; n_mels * frames_m.len()];
    for m in 0..n_mels {
        for f in 0..frames_m.len() {
            let mut sum = 0.0;
            for b in 0..N_BINS {
                sum += fbank[m][b] * mag_m[b * frames_m.len() + f];
            }
            mel_m[m * frames_m.len() + f] = sum;
        }
    }

    // Fit H-only for music
    let mut init_h_m = vec![0.0f32; k * frames_m.len()];
    for x in init_h_m.iter_mut() {
        *x = next_rand();
    }

    // To freeze W in a test, we can just compute the cost with lambda = conv_model
    // wait, we need to FIT H to get the best cost! I will write a small H-only fit loop.
    let (_, cost_music) = sp314_dsp::stft::nmfd::nmfd_f32_h_only(
        &mel_m,
        &tensor_w,
        &init_h_m,
        n_mels,
        k,
        frames_m.len(),
        tau,
        num_iter,
    );

    println!("WSPEECH|COST|speech={}|music={}", cost_speech, cost_music);
}
