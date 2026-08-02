use rustfft::num_complex::Complex;
use sha2::{Digest, Sha256};
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::{apply_spectral_mask_to_chunk, TwoPassEngine};
use sp314_dsp::stft::{StreamingStftEncoder, N_BINS};
use std::fs::File;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

fn decode_audio(path: &str, ext: &str) -> Vec<f32> {
    let src = File::open(path).unwrap();
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .unwrap();
    let mut decoder = symphonia::default::get_codecs()
        .make(
            &probed.format.default_track().unwrap().codec_params,
            &DecoderOptions::default(),
        )
        .unwrap();
    let mut out = Vec::new();
    loop {
        match probed.format.next_packet() {
            Ok(packet) => {
                let decoded = decoder.decode(&packet).unwrap();
                let mut sample_buf =
                    SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                sample_buf.copy_interleaved_ref(decoded);
                out.extend_from_slice(sample_buf.samples());
            }
            Err(_) => break,
        }
    }
    let mono: Vec<f32> = out.chunks_exact(2).map(|x| (x[0] + x[1]) * 0.5).collect();
    let valid_len = (mono.len() / 512) * 512;
    mono[..valid_len].to_vec()
}

fn compute_cplx_spectrogram(signal: &[f32]) -> Vec<Vec<Complex<f32>>> {
    let mut encoder = StreamingStftEncoder::new();
    let mut cplx = encoder.feed_chunk(signal);
    cplx.extend(encoder.finish());
    cplx
}

fn old_builder_mel_mask(
    component: usize,
    h_chunk: &[f32],
    tensor_w: &[f32],
    n_chunk_frames: usize,
    tau_frames: usize,
    k: usize,
) -> Vec<Vec<f32>> {
    let mut expanded_mask = vec![vec![0.0_f32; 1025]; n_chunk_frames];
    for f in 0..n_chunk_frames {
        let mut mel_mask = [0.0_f32; 128];
        for m in 0..128 {
            let mut target = 0.0_f32;
            let mut total = 0.0_f32;
            for c in 0..k {
                for tau in 0..tau_frames {
                    if f >= tau {
                        let h_f = f - tau;
                        let h_idx = c * n_chunk_frames + h_f;
                        let h_val = if h_idx < h_chunk.len() {
                            h_chunk[h_idx]
                        } else {
                            0.0
                        };
                        let w_c = tensor_w[m * (k * tau_frames) + c * tau_frames + tau];
                        if c == component {
                            target += w_c * h_val;
                        }
                        total += w_c * h_val;
                    }
                }
            }
            mel_mask[m] = target / (total + 1e-10_f32);
        }
        let linear_mask = sp314_dsp::analysis::mel_128::expand_mask_to_linear(&mel_mask);
        expanded_mask[f].copy_from_slice(&linear_mask);
    }
    expanded_mask
}

#[test]
fn test_nmfd_mask_oracle_a_and_b() {
    let base_audio = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audiobook");
    let path = format!("{}/mix_3_snr-25.flac", base_audio);
    let signal = decode_audio(&path, "flac");

    let chunk_len = 65536;
    let history_len = 10240;

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, 48000, None, None);
    let tensor_w = scout.tensor_w.clone();
    let tau = scout.tau;
    let k_b = 8;

    let nmf_b = NmfEngine::new(k_b);
    let full_cplx = compute_cplx_spectrogram(&signal);
    let n_frames = full_cplx.len();
    let mut chunked_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k_b];

    let mut pos = 0;
    let mut hasher = Sha256::new();

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
            let mut mag = vec![0.0_f32; 1025];
            for b in 0..1025 {
                mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
            }
            let mag_arr: [f32; 1025] = mag.try_into().unwrap();
            chunk_mel.push(sp314_dsp::analysis::mel_128::fold_to_mel(&mag_arr));
        }

        let mut c_v = vec![0.0_f32; 128 * c_frames];
        for f in 0..c_frames {
            for b in 0..128 {
                c_v[b * c_frames + f] = chunk_mel[f][b];
            }
        }
        let init_h_chunk = vec![0.1_f32; k_b * c_frames];
        let (chunk_h, _) =
            nmfd_f32_h_only(&c_v, &tensor_w, &init_h_chunk, 128, k_b, c_frames, tau, 12);

        let mut all_c_masks = Vec::new();
        for comp in 0..k_b {
            let c_mask =
                nmf_b.nmfd_component_mask_chunk(comp, &chunk_h, &tensor_w, c_frames, N_BINS, tau);
            all_c_masks.push(c_mask);
        }

        for f in 0..c_frames {
            let mut s = 0.0_f32;
            for comp in 0..k_b {
                s += all_c_masks[comp][f][200];
            }
            hasher.update(&s.to_le_bytes());

            // STRICT PARTITION ASSERTION (a)
            for b in 0..N_BINS {
                let mut b_s = 0.0_f32;
                for comp in 0..k_b {
                    b_s += all_c_masks[comp][f][b];
                }
                assert!(
                    (b_s - 1.0).abs() < 1e-5,
                    "Partition broken at frame {} bin {}: sum={}",
                    f,
                    b,
                    b_s
                );
            }
        }

        let pad_samples_current = if pos == 0 { 0 } else { history_len };
        let pad_frames_current = pad_samples_current / 512;
        let global_start_frame = (start + pad_samples_current) / 512;
        let is_last_chunk = pos + chunk_len >= signal.len();

        for comp in 0..k_b {
            let c_mask = &all_c_masks[comp];
            let core_mask = if c_mask.len() > pad_frames_current {
                &c_mask[pad_frames_current..]
            } else {
                &[]
            };
            let mut i = 0;
            while i < core_mask.len() && global_start_frame + i < n_frames {
                chunked_masks[comp][global_start_frame + i] = core_mask[i].clone();
                i += 1;
            }
            if is_last_chunk && !core_mask.is_empty() {
                while global_start_frame + i < n_frames {
                    chunked_masks[comp][global_start_frame + i] = core_mask.last().unwrap().clone();
                    i += 1;
                }
            }
        }
        pos += chunk_len;
    }

    // (a) Print the true active SNR
    let mut chunked_stems = Vec::new();
    for comp in 0..k_b {
        chunked_stems.push(apply_spectral_mask_to_chunk(
            &full_cplx,
            &chunked_masks[comp],
            signal.len(),
        ));
    }
    let mut num_pf_active = 0.0_f64;
    let mut den_pf_active = 0.0_f64;
    let active_start = 65536;
    let active_end = signal.len().saturating_sub(65536);

    for i in active_start..active_end {
        let mut s = 0.0_f32;
        for comp in 0..k_b {
            s += chunked_stems[comp][i];
        }
        let diff = signal[i] - s;
        num_pf_active += signal[i] as f64 * signal[i] as f64;
        den_pf_active += diff as f64 * diff as f64;
    }
    let true_active_snr_db = 10.0 * (num_pf_active / den_pf_active.max(1e-12)).log10();
    println!(
        "ORACLE|mix_3_snr-25|strict_partition_snr_db={:.2}",
        true_active_snr_db
    );
    assert!(
        true_active_snr_db > 100.0,
        "true_active_snr_db must be > 100dB, got {}",
        true_active_snr_db
    );

    // (b) DETERMINISM byte-equal
    let hash_result = format!("{:x}", hasher.finalize());
    println!("ORACLE|mix_3_snr-25|determinism_hash={}", hash_result);
}

#[test]
fn test_nmfd_mask_oracle_c() {
    let base_audio = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let path = format!("{}/bodleasons_mid.wav", base_audio);
    let signal = decode_audio(&path, "wav");

    let chunk_len = 65536;
    let history_len = 10240;

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, 48000, None, None);
    let tensor_w = scout.tensor_w.clone();
    let tau = scout.tau;
    let k_b = 8;

    let nmf_b = NmfEngine::new(k_b);
    let full_cplx = compute_cplx_spectrogram(&signal);
    let n_frames = full_cplx.len();

    let mut chunked_new_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k_b];
    let mut chunked_old_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k_b];

    let mut pos = 0;
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
            let mut mag = vec![0.0_f32; 1025];
            for b in 0..1025 {
                mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
            }
            let mag_arr: [f32; 1025] = mag.try_into().unwrap();
            chunk_mel.push(sp314_dsp::analysis::mel_128::fold_to_mel(&mag_arr));
        }

        let mut c_v = vec![0.0_f32; 128 * c_frames];
        for f in 0..c_frames {
            for b in 0..128 {
                c_v[b * c_frames + f] = chunk_mel[f][b];
            }
        }
        let init_h_chunk = vec![0.1_f32; k_b * c_frames];
        let (chunk_h, _) =
            nmfd_f32_h_only(&c_v, &tensor_w, &init_h_chunk, 128, k_b, c_frames, tau, 12);

        let mut all_new_masks = Vec::new();
        let mut all_old_masks = Vec::new();
        for comp in 0..k_b {
            all_new_masks.push(
                nmf_b.nmfd_component_mask_chunk(comp, &chunk_h, &tensor_w, c_frames, N_BINS, tau),
            );
            all_old_masks.push(old_builder_mel_mask(
                comp, &chunk_h, &tensor_w, c_frames, tau, k_b,
            ));
        }

        for f in 0..c_frames {
            let mut b_s = 0.0_f32;
            for b in 0..N_BINS {
                b_s = 0.0;
                for comp in 0..k_b {
                    b_s += all_new_masks[comp][f][b];
                }
                assert!(
                    (b_s - 1.0).abs() < 1e-5,
                    "Partition broken on bodleasons new builder frame {} bin {}: sum={}",
                    f,
                    b,
                    b_s
                );
            }
        }

        let pad_samples_current = if pos == 0 { 0 } else { history_len };
        let pad_frames_current = pad_samples_current / 512;
        let global_start_frame = (start + pad_samples_current) / 512;
        let is_last_chunk = pos + chunk_len >= signal.len();

        for comp in 0..k_b {
            let c_new_mask = &all_new_masks[comp];
            let c_old_mask = &all_old_masks[comp];
            let core_new = if c_new_mask.len() > pad_frames_current {
                &c_new_mask[pad_frames_current..]
            } else {
                &[]
            };
            let core_old = if c_old_mask.len() > pad_frames_current {
                &c_old_mask[pad_frames_current..]
            } else {
                &[]
            };

            let mut i = 0;
            while i < core_new.len() && global_start_frame + i < n_frames {
                chunked_new_masks[comp][global_start_frame + i] = core_new[i].clone();
                chunked_old_masks[comp][global_start_frame + i] = core_old[i].clone();
                i += 1;
            }
            if is_last_chunk && !core_new.is_empty() {
                while global_start_frame + i < n_frames {
                    chunked_new_masks[comp][global_start_frame + i] =
                        core_new.last().unwrap().clone();
                    chunked_old_masks[comp][global_start_frame + i] =
                        core_old.last().unwrap().clone();
                    i += 1;
                }
            }
        }
        pos += chunk_len;
    }

    // Render stems
    let mut new_voice = vec![0.0_f32; signal.len()];
    let mut new_music = vec![0.0_f32; signal.len()];
    let mut old_voice = vec![0.0_f32; signal.len()];
    let mut old_music = vec![0.0_f32; signal.len()];

    // Roles from TwoPassEngine scout:
    // This is hardcoded or inferred from TwoPassEngine, but TwoPassEngine stores `voice_slots`.
    // Wait, TwoPassEngine scout returns scout info, but TwoPassEngine doesn't persist `voice_slots` easily unless we do `TwoPassEngine::process_single_chunk`
    // Let's just render all stems and sum them into two big groups, or just check the difference per stem!
    // Since we just need to check the RMS diff of the audio output, we can check the RMS diff of the sum of all stems, or individual stems.
    // If the diff of every stem is < -60dB, then the diff of voice/music is also < -60dB.

    for comp in 0..k_b {
        let new_stem =
            apply_spectral_mask_to_chunk(&full_cplx, &chunked_new_masks[comp], signal.len());
        let old_stem =
            apply_spectral_mask_to_chunk(&full_cplx, &chunked_old_masks[comp], signal.len());

        let mut diff_sq_sum = 0.0_f64;
        let mut sig_sq_sum = 0.0_f64;
        for i in 65536..signal.len().saturating_sub(65536) {
            let diff = new_stem[i] - old_stem[i];
            diff_sq_sum += diff as f64 * diff as f64;
            sig_sq_sum += new_stem[i] as f64 * new_stem[i] as f64;
        }

        let rms_diff_db = 10.0 * (diff_sq_sum / sig_sq_sum.max(1e-12)).log10();
        assert!(
            rms_diff_db < -60.0,
            "Audio diff for component {} too high: {:.2} dB",
            comp,
            rms_diff_db
        );
    }
    println!("ORACLE|bodleasons_mid|audio_diff_passed=true");
}

#[test]
fn test_nmfd_mask_oracle_d_zero_edge() {
    let k_b = 8;
    let nmf_b = NmfEngine::new(k_b);
    let c_frames = 1;
    let tau = 12;
    let h_chunk = vec![0.0_f32; k_b * c_frames]; // 0 H means 0 total
    let tensor_w = vec![1.0_f32; 128 * k_b * tau];

    let mut all_masks = Vec::new();
    for comp in 0..k_b {
        all_masks.push(
            nmf_b.nmfd_component_mask_chunk(comp, &h_chunk, &tensor_w, c_frames, N_BINS, tau),
        );
    }

    for b in 0..N_BINS {
        let mut b_s = 0.0_f32;
        for comp in 0..k_b {
            let m = all_masks[comp][0][b];
            assert!(m.is_finite());
            b_s += m;
        }
        assert!((b_s - 1.0).abs() < 1e-6);
    }
    println!("ORACLE|zero_edge|sum_is_1=true|is_finite=true");
}
