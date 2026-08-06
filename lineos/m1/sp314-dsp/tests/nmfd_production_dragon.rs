use rustfft::num_complex::Complex;
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::{StreamingStftEncoder, N_BINS};
use std::fs::File;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

fn decode_audio(path: &str) -> Vec<f32> {
    let src = File::open(path).unwrap();
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("flac");
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

#[test]
fn test_nmfd_production_dragon() {
    let base_audio = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audiobook");
    let path = format!("{}/mix_3_snr-25.flac", base_audio);
    let signal = decode_audio(&path);

    let chunk_len = 65536;
    let history_len = 10240;

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, 48000, None, None, true);
    let tensor_w = scout.tensor_w.clone();
    let tau = scout.tau;
    let k_b = 8;

    let nmf_b = NmfEngine::new(k_b);

    let mut flat_num = 0.0_f64;
    let mut flat_den = 0.0_f64;
    let mut weighted_num = 0.0_f64;
    let mut weighted_den = 0.0_f64;

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

        let mut all_c_masks = Vec::new();
        for comp in 0..k_b {
            let c_mask =
                nmf_b.nmfd_component_mask_chunk(comp, &chunk_h, &tensor_w, c_frames, N_BINS, tau);
            all_c_masks.push(c_mask);
        }

        let pad_frames = if pos == 0 { 0 } else { history_len / 512 };

        for f in pad_frames..c_frames {
            // Check if this global frame is within the active region
            let global_f = (start / 512) + f;
            let active_start_f = 65536 / 512;
            let active_end_f = (signal.len() - 65536) / 512;

            if global_f >= active_start_f && global_f < active_end_f {
                for b in 0..N_BINS {
                    let mut s_mask = 0.0_f32;
                    for comp in 0..k_b {
                        s_mask += all_c_masks[comp][f][b];
                    }
                    let mask_err = (1.0 - s_mask as f64).abs();

                    flat_num += 1.0;
                    flat_den += mask_err * mask_err;

                    let bin_energy =
                        (chunk_cplx[f][b].re as f64).powi(2) + (chunk_cplx[f][b].im as f64).powi(2);
                    weighted_num += bin_energy;
                    weighted_den += bin_energy * mask_err * mask_err;
                }
            }
        }
        pos += chunk_len;
    }

    let flat_snr_db = 10.0 * (flat_num / flat_den.max(1e-12)).log10();
    let energy_weighted_snr_db = 10.0 * (weighted_num / weighted_den.max(1e-12)).log10();

    println!(
        "PART_CMP|file=mix_3_snr-25|flat_snr_db={:.2}|energy_weighted_snr_db={:.2}",
        flat_snr_db, energy_weighted_snr_db
    );
}
