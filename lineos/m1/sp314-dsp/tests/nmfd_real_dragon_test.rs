use hound;
use rustfft::num_complex::Complex;
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::apply_spectral_mask_to_chunk;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::{StreamingStftEncoder, N_BINS};

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

#[test]
fn test_dragon_test() {
    let signal = read_audio("tests/fixtures/bodleasons_mid.wav");

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, &signal, 48000, None, None, true);

    let tensor_w = scout.tensor_w.clone();
    let tau = scout.tau;
    let k = 5;

    println!("=== COLD INIT ===");
    run_dragon_audio(&signal, &tensor_w, &scout, k, tau, false);
    println!("=== PROJECTION INIT ===");
    run_dragon_audio(&signal, &tensor_w, &scout, k, tau, true);
}

fn projection_init(v: &[f32], tensor_w: &[f32], k: usize, n_frames: usize, tau: usize) -> Vec<f32> {
    let mut init_h = vec![0.0_f32; k * n_frames];
    for f in 0..n_frames {
        let mut sum = 0.0_f32;
        for c in 0..k {
            let mut proj = 0.0_f32;
            for b in 0..128 {
                let w_val = tensor_w[b * k * tau + c * tau];
                proj += v[b * n_frames + f] * w_val;
            }
            init_h[c * n_frames + f] = proj;
            sum += proj;
        }
        if sum < 1e-10 {
            sum = 1e-10;
        }
        for c in 0..k {
            init_h[c * n_frames + f] = (init_h[c * n_frames + f] / sum).max(1e-10);
        }
    }
    init_h
}

fn rms_db(signal: &[f32]) -> f32 {
    if signal.is_empty() {
        return -144.0;
    }
    let mut sum = 0.0;
    for &x in signal {
        sum += x * x;
    }
    let rms = (sum / signal.len() as f32).sqrt();
    20.0 * (rms + 1e-9).log10()
}

fn measure_region(
    whole: &[f32],
    chunked: &[f32],
    start: usize,
    end: usize,
    name: &str,
    stem: &str,
) {
    let mut max_abs = 0.0_f32;
    let mut delta = Vec::with_capacity(end - start);
    for i in start..end {
        let d = whole[i] - chunked[i];
        if d.abs() > max_abs {
            max_abs = d.abs();
        }
        delta.push(d);
    }
    let rms_d = rms_db(&delta);
    println!(
        "NMFDAUDIO|{}|stem={}|max_abs={:.6}|rms_db_delta={:.2}",
        name, stem, max_abs, rms_d
    );
}

fn run_dragon_audio(
    signal: &[f32],
    tensor_w: &[f32],
    scout: &sp314_dsp::stft::two_pass::ScoutResult,
    k: usize,
    tau: usize,
    use_projection: bool,
) {
    let full_cplx = compute_cplx_spectrogram(signal);
    let n_frames = full_cplx.len();

    let mut mel_frames = Vec::new();
    for f in &full_cplx {
        let mut mag = [0.0_f32; 1025];
        for b in 0..1025 {
            mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
        }
        mel_frames.push(sp314_dsp::analysis::mel_128::fold_to_mel(&mag));
    }

    let mut full_v = vec![0.0_f32; 128 * n_frames];
    for f in 0..n_frames {
        for b in 0..128 {
            full_v[b * n_frames + f] = mel_frames[f][b];
        }
    }

    let init_h_full = if use_projection {
        projection_init(&full_v, tensor_w, k, n_frames, tau)
    } else {
        vec![0.1_f32; k * n_frames]
    };

    let (whole_h, _) = nmfd_f32_h_only(&full_v, tensor_w, &init_h_full, 128, k, n_frames, tau, 12);

    let nmf = NmfEngine::new(k);
    let whole_voice_mask =
        nmf.nmfd_component_mask_chunk(scout.voice_idx, &whole_h, tensor_w, n_frames, N_BINS, tau);
    let whole_amb_mask = nmf.nmfd_component_mask_chunk(
        scout.ambience_idx,
        &whole_h,
        tensor_w,
        n_frames,
        N_BINS,
        tau,
    );

    let chunk_len = 96000;
    let history_len = 10240;

    let mut chunked_voice_mask: Vec<Vec<f32>> = vec![vec![0.0_f32; N_BINS]; n_frames];
    let mut chunked_amb_mask: Vec<Vec<f32>> = vec![vec![0.0_f32; N_BINS]; n_frames];

    let mut pos = 0;
    let mut num_boundaries = 0;

    while pos < signal.len() {
        let start = if pos > history_len {
            pos - history_len
        } else {
            0
        };
        let end = std::cmp::min(pos + chunk_len, signal.len());

        let chunk_signal = &signal[start..end];
        let chunk_cplx = compute_cplx_spectrogram(chunk_signal);
        let c_frames = chunk_cplx.len();

        let mut chunk_mel = Vec::new();
        for f in &chunk_cplx {
            let mut mag = [0.0_f32; 1025];
            for b in 0..1025 {
                mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
            }
            chunk_mel.push(sp314_dsp::analysis::mel_128::fold_to_mel(&mag));
        }

        let mut c_v = vec![0.0_f32; 128 * c_frames];
        for f in 0..c_frames {
            for b in 0..128 {
                c_v[b * c_frames + f] = chunk_mel[f][b];
            }
        }

        let init_h_chunk = if use_projection {
            projection_init(&c_v, tensor_w, k, c_frames, tau)
        } else {
            vec![0.1_f32; k * c_frames]
        };

        let (chunk_h, _) =
            nmfd_f32_h_only(&c_v, tensor_w, &init_h_chunk, 128, k, c_frames, tau, 12);

        let cv_mask = nmf.nmfd_component_mask_chunk(
            scout.voice_idx,
            &chunk_h,
            tensor_w,
            c_frames,
            N_BINS,
            tau,
        );
        let ca_mask = nmf.nmfd_component_mask_chunk(
            scout.ambience_idx,
            &chunk_h,
            tensor_w,
            c_frames,
            N_BINS,
            tau,
        );

        let pad_frames = if pos == 0 {
            0
        } else {
            (history_len + 1024) / 512
        };
        let start_frame = if pos == 0 {
            0
        } else {
            num_boundaries * (chunk_len / 512)
        };

        let core_cv = if cv_mask.len() > pad_frames {
            &cv_mask[pad_frames..]
        } else {
            &[]
        };
        for i in 0..core_cv.len() {
            if start_frame + i < n_frames {
                chunked_voice_mask[start_frame + i] = core_cv[i].clone();
            }
        }

        let core_ca = if ca_mask.len() > pad_frames {
            &ca_mask[pad_frames..]
        } else {
            &[]
        };
        for i in 0..core_ca.len() {
            if start_frame + i < n_frames {
                chunked_amb_mask[start_frame + i] = core_ca[i].clone();
            }
        }

        if pos > 0 {
            num_boundaries += 1;
        }
        pos += chunk_len;
    }

    // Audio generation
    let audio_whole_voice =
        apply_spectral_mask_to_chunk(&full_cplx, &whole_voice_mask, signal.len());
    let audio_chunked_voice =
        apply_spectral_mask_to_chunk(&full_cplx, &chunked_voice_mask, signal.len());

    let audio_whole_amb = apply_spectral_mask_to_chunk(&full_cplx, &whole_amb_mask, signal.len());
    let audio_chunked_amb =
        apply_spectral_mask_to_chunk(&full_cplx, &chunked_amb_mask, signal.len());

    // Evaluate
    // Control region (mid chunk 0)
    let c0 = 48000;
    measure_region(
        &audio_whole_voice,
        &audio_chunked_voice,
        c0 - 2048,
        c0 + 2048,
        "control=0",
        "voice",
    );
    measure_region(
        &audio_whole_amb,
        &audio_chunked_amb,
        c0 - 2048,
        c0 + 2048,
        "control=0",
        "ambience",
    );

    for b_idx in 0..num_boundaries {
        let b_samp = (b_idx + 1) * chunk_len;
        if b_samp + 2048 <= signal.len() {
            let start = b_samp - 2048;
            let end = b_samp + 2048;
            let name = format!("boundary={}", b_idx);
            measure_region(
                &audio_whole_voice,
                &audio_chunked_voice,
                start,
                end,
                &name,
                "voice",
            );
            measure_region(
                &audio_whole_amb,
                &audio_chunked_amb,
                start,
                end,
                &name,
                "ambience",
            );
        }
    }

    measure_region(
        &audio_whole_voice,
        &audio_chunked_voice,
        0,
        signal.len(),
        "global_worst",
        "voice",
    );
    measure_region(
        &audio_whole_amb,
        &audio_chunked_amb,
        0,
        signal.len(),
        "global_worst",
        "ambience",
    );
}
