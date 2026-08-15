use hound;
use rustfft::num_complex::Complex;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::two_pass::DRUM_DEDUP_ALPHA;
use sp314_dsp::stft::{StreamingStftEncoder, N_BINS};
use sp314_dsp::stft::hpss::HpssProcessor;

fn read_audio(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let samples: Vec<i16> = reader.samples().map(|s| s.unwrap()).collect();
    let mut mono = Vec::new();
    if spec.channels == 2 {
        for i in 0..(samples.len() / 2) {
            let l = samples[2 * i] as f32 / 32768.0;
            let r = samples[2 * i + 1] as f32 / 32768.0;
            mono.push((l + r) * 0.5);
        }
    } else {
        for s in samples {
            mono.push(s as f32 / 32768.0);
        }
    }
    mono
}

fn compute_cplx_spectrogram(signal: &[f32]) -> Vec<Vec<Complex<f32>>> {
    let mut encoder = StreamingStftEncoder::new();
    let mut cplx = encoder.feed_chunk(signal);
    cplx.extend(encoder.finish());
    cplx
}

fn rms(signal: &[f32]) -> f32 {
    if signal.is_empty() { return 0.0; }
    let mut sum = 0.0;
    for &x in signal { sum += x * x; }
    (sum / signal.len() as f32).sqrt()
}

fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n < 2 { return 0.0; }
    let mean_a = a.iter().sum::<f32>() / n as f32;
    let mean_b = b.iter().sum::<f32>() / n as f32;
    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    for i in 0..n {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }
    if var_a * var_b > 0.0 {
        cov / (var_a * var_b).sqrt()
    } else {
        0.0
    }
}

#[test]
fn test_dedup_guard() {
    let signal = read_audio("tests/fixtures/bodleasons_mid.wav");
    
    // Calculate mean(core_mask_p) using HPSS independently
    let full_cplx = compute_cplx_spectrogram(&signal);
    let n_frames = full_cplx.len();
    let mut magnitudes = vec![vec![0.0_f32; N_BINS]; n_frames];
    for t in 0..n_frames {
        for b in 0..N_BINS {
            let re = full_cplx[t][b].re;
            let im = full_cplx[t][b].im;
            magnitudes[t][b] = (re * re + im * im).sqrt();
        }
    }
    let mut hpss = HpssProcessor::new();
    let (_, mask_p) = hpss.process(&magnitudes);
    
    let mut sum_p = 0.0;
    for t in 0..n_frames {
        for b in 0..N_BINS {
            sum_p += mask_p[t][b];
        }
    }
    let mean_core_mask_p = sum_p / (n_frames * N_BINS) as f32;

    println!("[DEDUP-GUARD] mean_core_mask_p: {:.4}, DRUM_DEDUP_ALPHA: {}", mean_core_mask_p, DRUM_DEDUP_ALPHA);

    // ASSERT E1: Guard exercise check
    if mean_core_mask_p <= 0.1 || DRUM_DEDUP_ALPHA <= 0.0 {
        panic!("guard not exercised: mean_core_mask_p ({:.4}) <= 0.1 OR DRUM_DEDUP_ALPHA ({}) <= 0.0", mean_core_mask_p, DRUM_DEDUP_ALPHA);
    }

    // Process via streaming path
    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, &signal, 48000, None, None, true);

    let mut out_voice = Vec::new();
    let mut out_drums = Vec::new();
    let mut out_bass = Vec::new();
    let mut out_harmonics = Vec::new();
    let mut out_ambience = Vec::new();

    engine.process_chunks(&signal, &scout, true, |chunk| {
        out_voice.extend_from_slice(&chunk.voice.mono());
        out_drums.extend_from_slice(&chunk.drums.mono());
        out_bass.extend_from_slice(&chunk.bass.mono());
        out_harmonics.extend_from_slice(&chunk.harmonics.mono());
        out_ambience.extend_from_slice(&chunk.ambience.mono());
    }).unwrap();

    let n = signal.len();
    let mut sum_stems = vec![0.0_f32; n];
    for i in 0..n {
        let v = *out_voice.get(i).unwrap_or(&0.0);
        let d = *out_drums.get(i).unwrap_or(&0.0);
        let b = *out_bass.get(i).unwrap_or(&0.0);
        let h = *out_harmonics.get(i).unwrap_or(&0.0);
        let a = *out_ambience.get(i).unwrap_or(&0.0);
        sum_stems[i] = v + d + b + h + a;
    }

    let rms_sum = rms(&sum_stems);
    let rms_mono = rms(&signal);
    let srr = rms_sum / rms_mono;
    let corr = pearson(&sum_stems, &signal);

    println!("[DEDUP-GUARD] srr: {:.4}", srr);
    println!("[DEDUP-GUARD] corr: {:.4}", corr);
    println!("[DEDUP-GUARD] rms(drums): {:.6}", rms(&out_drums));

    assert!((srr - 1.2424).abs() <= 0.02, "SRR outside measured range"); // measured 2026-08-15 @ alpha=0.5, bca262a
    assert!(corr > 0.97, "Corr dropped below 0.97"); // measured 0.9772 2026-08-15 @ alpha=0.5, bca262a
    assert!(rms(&out_drums) > 0.0, "Drums are empty"); // measured rms(drums) = 0.098944
}
