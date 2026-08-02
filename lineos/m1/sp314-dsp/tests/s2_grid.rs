use std::fs::File;
use std::io::Read;
use serde_json::Value;
use std::collections::HashMap;

use sp314_dsp::analysis::onset_flux::SuperFluxOnset;
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::{apply_spectral_mask_to_chunk, TwoPassEngine};
use sp314_dsp::stft::{StreamingStftEncoder, N_BINS};

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use rustfft::num_complex::Complex;

fn decode_audio(path: &str) -> Vec<f32> {
    let src = File::open(path).expect("Failed to open file");
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("flac");

    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();
    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .expect("Failed to probe");

    let mut decoder = symphonia::default::get_codecs()
        .make(&probed.format.default_track().unwrap().codec_params, &DecoderOptions::default())
        .expect("Failed to make decoder");

    let mut out = Vec::new();
    loop {
        match probed.format.next_packet() {
            Ok(packet) => {
                let decoded = decoder.decode(&packet).unwrap();
                let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                sample_buf.copy_interleaved_ref(decoded);
                out.extend_from_slice(sample_buf.samples());
            }
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(_) => break,
        }
    }
    out.chunks_exact(2).map(|x| (x[0] + x[1]) * 0.5).collect()
}

fn load_pmap(path: &str) -> Vec<f32> {
    let mut file = File::open(path).expect("Failed to open pmap");
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let v: Value = serde_json::from_str(&contents).unwrap();
    let probs = v["probs"].as_array().unwrap();
    probs.iter().map(|p| p.as_f64().unwrap() as f32).collect()
}

fn compute_cplx_spectrogram(signal: &[f32]) -> Vec<Vec<Complex<f32>>> {
    let mut encoder = StreamingStftEncoder::new();
    let mut cplx = encoder.feed_chunk(signal);
    cplx.extend(encoder.finish());
    cplx
}

#[derive(Default)]
struct SummaryStats {
    suppress_sum: f64,
    preserve_sum: f64,
    peaks_diff_sum: f64,
    count: usize,
}

#[test]
fn test_s2_grid() {
    let base_audio = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audiobook");
    let base_pmaps = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../research/silero-lab/pmaps");

    let target_mixes = vec!["mix_1_snr-6", "mix_2_snr-6", "mix_3_snr-6"];
    let floors = vec![0.30_f32, 0.20, 0.10, 0.05];
    let ramps = vec![6, 9];
    
    let mut summary = HashMap::new();

    for mix_name in &target_mixes {
        let voice_id = match mix_name.split("_").nth(1).unwrap() {
            "1" => "voice_1",
            "2" => "voice_2",
            "3" => "voice_3",
            _ => panic!("Unknown mix"),
        };
        let truth_audio_path = format!("{}/{}.flac", base_audio, voice_id);
        let truth_signal = decode_audio(&truth_audio_path);
        
        let truth_pmap_path = format!("{}/{}.flac.pmap.json", base_pmaps, voice_id);
        let truth_pmap = load_pmap(&truth_pmap_path);

        let audio_path = format!("{}/{}.flac", base_audio, mix_name);
        let silero_pmap_path = format!("{}/{}.flac.pmap.json", base_pmaps, mix_name);
        let pmap = load_pmap(&silero_pmap_path);

        let signal = decode_audio(&audio_path);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, 48000, None, None);

        let tensor_w = scout.tensor_w.clone();
        let tau = scout.tau;
        let k_b = 8;

        let full_cplx = compute_cplx_spectrogram(&signal);
        let n_frames = full_cplx.len();

        let mut nmf_b = NmfEngine::new(k_b);
        let mut original_b_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k_b];
        
        let chunk_len = 96000;
        let history_len = 10240;
        let mut pos = 0;

        while pos < signal.len() {
            let start = if pos > history_len { pos - history_len } else { 0 };
            let end = std::cmp::min(pos + chunk_len, signal.len());
            let is_last_chunk = pos + chunk_len >= signal.len();
            let chunk_signal = &signal[start..end];
            let chunk_cplx = compute_cplx_spectrogram(chunk_signal);
            let c_frames = chunk_cplx.len();

            let mut chunk_mel = Vec::new();
            for f in &chunk_cplx {
                let mut mag = vec![0.0_f32; 1025];
                for b in 0..1025 { mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im); }
                let mag_arr: [f32; 1025] = mag.try_into().unwrap();
                chunk_mel.push(sp314_dsp::analysis::mel_128::fold_to_mel(&mag_arr));
            }

            let pad_samples = if pos == 0 { 0 } else { history_len };
            let pad_frames = pad_samples / 512;
            let global_start_frame = (start + pad_samples) / 512;

            let mut c_v = vec![0.0_f32; 128 * c_frames];
            for f in 0..c_frames {
                for b in 0..128 { c_v[b * c_frames + f] = chunk_mel[f][b]; }
            }
            let init_h_chunk = vec![0.1_f32; k_b * c_frames];
            let (chunk_h, _) = nmfd_f32_h_only(&c_v, &tensor_w, &init_h_chunk, 128, k_b, c_frames, tau, 12);

            for comp in 0..k_b {
                let c_mask = nmf_b.nmfd_component_mask_chunk(comp, &chunk_h, &tensor_w, c_frames, N_BINS, tau);
                let core_mask = if c_mask.len() > pad_frames { &c_mask[pad_frames..] } else { &[] };
                let mut i = 0;
                while i < core_mask.len() && global_start_frame + i < n_frames {
                    original_b_masks[comp][global_start_frame + i] = core_mask[i].clone();
                    i += 1;
                }
                if is_last_chunk && !core_mask.is_empty() {
                    while global_start_frame + i < n_frames {
                        original_b_masks[comp][global_start_frame + i] = core_mask.last().unwrap().clone();
                        i += 1;
                    }
                }
            }
            pos += chunk_len;
        }

        let mut b_stems_raw = Vec::new();
        for comp in 0..k_b { b_stems_raw.push(apply_spectral_mask_to_chunk(&full_cplx, &original_b_masks[comp], signal.len())); }
        let mut b_voice_ungated = vec![0.0_f32; signal.len()];
        for i in 0..signal.len() {
            b_voice_ungated[i] = b_stems_raw[0][i] + b_stems_raw[1][i] + b_stems_raw[2][i] + b_stems_raw[3][i];
        }

        for &floor_val in &floors {
            let mut gate_target = vec![0.0_f32; n_frames];
            for f in 0..n_frames {
                let pmap_idx = std::cmp::min(f / 3, pmap.len().saturating_sub(1));
                let p = pmap[pmap_idx];
                gate_target[f] = p.max(floor_val).min(1.0);
            }

            for &n_ramp in &ramps {
                let mut smoothed_gate = gate_target.clone();
                let mut f_idx = 0;
                let mut gate_transitions = Vec::new();
                while f_idx < n_frames - 1 {
                    if gate_target[f_idx] != gate_target[f_idx+1] {
                        gate_transitions.push(f_idx * 512);
                        let start_val = gate_target[f_idx];
                        let end_val = gate_target[f_idx+1];
                        for i in 1..=n_ramp {
                            if f_idx + i < n_frames {
                                let alpha = i as f32 / n_ramp as f32;
                                smoothed_gate[f_idx + i] = start_val + (end_val - start_val) * alpha;
                            }
                        }
                        f_idx += n_ramp;
                    } else {
                        f_idx += 1;
                    }
                }
                
                let b_boundaries = gate_transitions.clone();
                let mut b_controls = Vec::new();
                for &b in &b_boundaries {
                    if b > 44100 { b_controls.push(b - 44100); }
                }
                
                let mut current_masks = original_b_masks.clone();
                for f in 0..n_frames {
                    let g = smoothed_gate[f];
                    if g < 1.0 {
                        for b in 0..N_BINS {
                            let mut non_voice_sum = 0.0_f32;
                            for comp in 4..k_b { non_voice_sum += current_masks[comp][f][b]; }
                            let mut voice_sum = 0.0_f32;
                            for comp in 0..4 { voice_sum += current_masks[comp][f][b]; }
                            
                            let target_non_voice_sum = 1.0 - voice_sum * g;
                            let multiplier = if non_voice_sum > 1e-12 {
                                target_non_voice_sum / non_voice_sum
                            } else { 1.0 };
                            
                            for comp in 0..4 { current_masks[comp][f][b] *= g; }
                            for comp in 4..k_b { current_masks[comp][f][b] *= multiplier; }
                        }
                    }
                }
                
                let mut b_stems_gated = Vec::new();
                for comp in 0..4 { b_stems_gated.push(apply_spectral_mask_to_chunk(&full_cplx, &current_masks[comp], signal.len())); }
                
                let mut b_voice_gated = vec![0.0_f32; signal.len()];
                for i in 0..signal.len() {
                    b_voice_gated[i] = b_stems_gated[0][i] + b_stems_gated[1][i] + b_stems_gated[2][i] + b_stems_gated[3][i];
                }
                
                let mut e_speech_gated = 0.0_f64;
                let mut e_speech_raw = 0.0_f64;
                let mut e_nospeech_gated = 0.0_f64;
                let mut e_nospeech_raw = 0.0_f64;
                for i in 0..signal.len() {
                    let pmap_idx = std::cmp::min((i as f32 / (48000.0 / 31.25)) as usize, truth_pmap.len().saturating_sub(1));
                    let speech = truth_pmap[pmap_idx] >= 0.5;
                    let g_val = b_voice_gated[i] as f64;
                    let r_val = b_voice_ungated[i] as f64;
                    if speech {
                        e_speech_gated += g_val * g_val;
                        e_speech_raw += r_val * r_val;
                    } else {
                        e_nospeech_gated += g_val * g_val;
                        e_nospeech_raw += r_val * r_val;
                    }
                }
                let db_speech_gated = 10.0 * (e_speech_gated.max(1e-12)).log10();
                let db_speech_raw = 10.0 * (e_speech_raw.max(1e-12)).log10();
                let db_nospeech_gated = 10.0 * (e_nospeech_gated.max(1e-12)).log10();
                let db_nospeech_raw = 10.0 * (e_nospeech_raw.max(1e-12)).log10();
                
                let suppress = db_nospeech_gated - db_nospeech_raw;
                let preserve = db_speech_gated - db_speech_raw;
                
                let mut num_gated = 0.0_f64;
                let mut den_gated = 0.0_f64;
                for i in 0..signal.len().min(truth_signal.len()) {
                    let t = truth_signal[i] as f64;
                    let g = b_voice_gated[i] as f64;
                    num_gated += t * t;
                    den_gated += (t - g) * (t - g);
                }
                let snr_gated = 10.0 * (num_gated / den_gated.max(1e-12)).log10();
                
                let mut envelope = Vec::new();
                let mut superflux = SuperFluxOnset::new(10.0);
                let mut encoder = StreamingStftEncoder::new();
                let mut cplx = encoder.feed_chunk(&b_voice_gated);
                cplx.extend(encoder.finish());
                for f in &cplx {
                    let mut mag = [0.0_f32; 1025];
                    for b in 0..1025 { mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im); }
                    envelope.push(superflux.process(&mag));
                }
                let mut peaks = vec![false; envelope.len()];
                for i in 1..envelope.len() - 1 {
                    if envelope[i] > envelope[i - 1] && envelope[i] > envelope[i + 1] && envelope[i] > 0.05 { peaks[i] = true; }
                }
                
                let mut b_peaks = 0;
                for &s in &b_boundaries {
                    if s + 2048 <= signal.len() {
                        let f = s / 512;
                        if f > 0 && f + 1 < peaks.len() {
                            if peaks[f - 1] || peaks[f] || peaks[f + 1] { b_peaks += 1; }
                        }
                    }
                }
                let mut c_peaks = 0;
                for &s in &b_controls {
                    let f = s / 512;
                    if f > 0 && f + 1 < peaks.len() {
                        if peaks[f - 1] || peaks[f] || peaks[f + 1] { c_peaks += 1; }
                    }
                }
                
                println!("GRID|floor={:.2}|n={}|mix={}|suppress_db={:.2}|preserve_db={:.2}|boundary_peaks={}|control_peaks={}|voice_snr_gated={:.2}", floor_val, n_ramp, mix_name, suppress, preserve, b_peaks, c_peaks, snr_gated);

                let key = format!("floor={:.2}|n={}", floor_val, n_ramp);
                let stats = summary.entry(key).or_insert(SummaryStats::default());
                stats.suppress_sum += suppress;
                stats.preserve_sum += preserve;
                stats.peaks_diff_sum += (b_peaks as f64) - (c_peaks as f64);
                stats.count += 1;
            }
        }
    }
    
    println!("\nSUMMARY:");
    let mut keys: Vec<_> = summary.keys().cloned().collect();
    // Sort keys logically by floor DESC then n ASC
    keys.sort_by(|a, b| b.cmp(a)); 
    
    for key in keys {
        let stats = &summary[&key];
        let m_suppress = stats.suppress_sum / stats.count as f64;
        let m_preserve = stats.preserve_sum / stats.count as f64;
        let m_diff = stats.peaks_diff_sum / stats.count as f64;
        println!("GRID_MEAN|{}|mean_suppress_db={:.2}|mean_preserve_db={:.2}|mean_peaks_diff={:.2}", key, m_suppress, m_preserve, m_diff);
    }
}
