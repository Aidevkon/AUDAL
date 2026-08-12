use hound;
use rustfft::num_complex::Complex;
use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{FixedPriors, VadClassifier};
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
fn test_permutation_suspect() {
    let signal = read_audio("tests/fixtures/bodleasons_mid.wav");
    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, &signal, 48000, None, None, true);

    let tensor_w = scout.tensor_w.clone();
    let proxy_w = scout.w.clone();
    let tau = scout.tau;
    let k = 5;

    let full_cplx = compute_cplx_spectrogram(&signal);
    let n_frames = full_cplx.len();

    let mut full_mag = Vec::new();
    let mut mel_frames = Vec::new();
    for f in &full_cplx {
        let mut mag = [0.0_f32; 1025];
        for b in 0..1025 {
            mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
        }
        full_mag.push(mag.to_vec());
        mel_frames.push(sp314_dsp::analysis::mel_128::fold_to_mel(&mag));
    }

    let mut full_v = vec![0.0_f32; 128 * n_frames];
    for f in 0..n_frames {
        for b in 0..128 {
            full_v[b * n_frames + f] = mel_frames[f][b];
        }
    }

    let mut nmf = NmfEngine::new(k);
    nmf.w = proxy_w.clone();

    // ==========================================
    // (B) NMFD chunked cold
    // ==========================================
    let mut b_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k];
    let mut b_h_sum = vec![0.0_f32; k];

    let chunk_len = 96000;
    let history_len = 10240;
    let mut pos = 0;
    while pos < signal.len() {
        let start = if pos > history_len {
            pos - history_len
        } else {
            0
        };
        let end = std::cmp::min(pos + chunk_len, signal.len());
        let is_last_chunk = pos + chunk_len >= signal.len();

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

        let pad_samples = if pos == 0 { 0 } else { history_len };
        let pad_frames = pad_samples / 512;
        let global_start_frame = (start + pad_samples) / 512;

        let mut c_v = vec![0.0_f32; 128 * c_frames];
        for f in 0..c_frames {
            for b in 0..128 {
                c_v[b * c_frames + f] = chunk_mel[f][b];
            }
        }
        let init_h_chunk = vec![0.1_f32; k * c_frames];
        let (chunk_h, _) =
            nmfd_f32_h_only(&c_v, &tensor_w, &init_h_chunk, 128, k, c_frames, tau, 12);

        for comp in 0..k {
            for f in 0..c_frames {
                b_h_sum[comp] += chunk_h[comp * c_frames + f];
            }
            let c_mask =
                nmf.nmfd_component_mask_chunk(comp, &chunk_h, &tensor_w, c_frames, N_BINS, tau);
            let core_mask = if c_mask.len() > pad_frames {
                &c_mask[pad_frames..]
            } else {
                &[]
            };

            let mut i = 0;
            while i < core_mask.len() && global_start_frame + i < n_frames {
                b_masks[comp][global_start_frame + i] = core_mask[i].clone();
                i += 1;
            }
            if is_last_chunk && !core_mask.is_empty() {
                while global_start_frame + i < n_frames {
                    b_masks[comp][global_start_frame + i] = core_mask.last().unwrap().clone();
                    i += 1;
                }
            }
        }
        pos += chunk_len;
    }

    let mut b_stems = Vec::new();
    for comp in 0..k {
        b_stems.push(apply_spectral_mask_to_chunk(
            &full_cplx,
            &b_masks[comp],
            signal.len(),
        ));
    }

    // ==========================================
    // (C) NMFD whole
    // ==========================================
    let mut c_h_sum = vec![0.0_f32; k];
    let init_h_full = vec![0.1_f32; k * n_frames];
    let (whole_h, _) = nmfd_f32_h_only(&full_v, &tensor_w, &init_h_full, 128, k, n_frames, tau, 12);
    let mut c_stems = Vec::new();
    for comp in 0..k {
        for f in 0..n_frames {
            c_h_sum[comp] += whole_h[comp * n_frames + f];
        }
        let whole_mask =
            nmf.nmfd_component_mask_chunk(comp, &whole_h, &tensor_w, n_frames, N_BINS, tau);
        c_stems.push(apply_spectral_mask_to_chunk(
            &full_cplx,
            &whole_mask,
            signal.len(),
        ));
    }

    // ==========================================
    // COMPUTE CENTROIDS
    // ==========================================
    let mut centroids = vec![0.0_f32; k];
    for comp in 0..k {
        let mut sum_w = 0.0_f32;
        let mut sum_mw = 0.0_f32;
        for m in 0..128 {
            let mut avg_t = 0.0_f32;
            for t in 0..tau {
                avg_t += tensor_w[m * (k * tau) + comp * tau + t];
            }
            avg_t /= tau as f32;
            sum_w += avg_t;
            sum_mw += m as f32 * avg_t;
        }
        centroids[comp] = sum_mw / sum_w.max(1e-12);
    }

    // ==========================================
    // REPORT
    // ==========================================
    let total_b_h: f32 = b_h_sum.iter().sum();
    let total_c_h: f32 = c_h_sum.iter().sum();

    println!("\n=== PERMUTATION SUSPECT: NMFD CHUNKED (B) ===");
    for comp in 0..k {
        let src_audio = &b_stems[comp] as &[f32];
        let mut ext = VadFeatureExtractor::new();
        let features = ext.process_chunk(src_audio, src_audio, src_audio);
        let mut vad = VadClassifier::new(FixedPriors);
        let mut speech_frames = 0;
        for f in &features {
            if vad.process(f, -144.0).is_speech {
                speech_frames += 1;
            }
        }
        let speech_pct = 100.0 * speech_frames as f32 / features.len().max(1) as f32;
        let h_share = 100.0 * b_h_sum[comp] / total_b_h.max(1e-12);

        let marker = if comp == scout.voice_idx {
            "<-- TARGET VOICE_IDX"
        } else {
            ""
        };
        println!(
            "PERM|variant=B|comp={}|centroid_band={:.2}|h_share={:.2}|vad_speech_pct={:.2} {}",
            comp, centroids[comp], h_share, speech_pct, marker
        );
    }

    println!("\n=== UNSUPERVISED SUSPECT: NMFD WHOLE (C) ===");
    for comp in 0..k {
        let src_audio = &c_stems[comp] as &[f32];
        let mut ext = VadFeatureExtractor::new();
        let features = ext.process_chunk(src_audio, src_audio, src_audio);
        let mut vad = VadClassifier::new(FixedPriors);
        let mut speech_frames = 0;
        for f in &features {
            if vad.process(f, -144.0).is_speech {
                speech_frames += 1;
            }
        }
        let speech_pct = 100.0 * speech_frames as f32 / features.len().max(1) as f32;
        let h_share = 100.0 * c_h_sum[comp] / total_c_h.max(1e-12);

        let marker = if comp == scout.voice_idx {
            "<-- TARGET VOICE_IDX"
        } else {
            ""
        };
        println!(
            "PERM|variant=C|comp={}|centroid_band={:.2}|h_share={:.2}|vad_speech_pct={:.2} {}",
            comp, centroids[comp], h_share, speech_pct, marker
        );
    }
}
