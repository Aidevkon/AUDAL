use hound;
use rustfft::num_complex::Complex;
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::hpss::HpssStreamContext;
use sp314_dsp::stft::{StftEngine, N_BINS};
use std::env;

fn read_audio(path: &str) -> (Vec<f32>, u32) {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    };
    let mut mono = Vec::new();
    if spec.channels == 2 {
        for i in 0..(samples.len() / 2) {
            mono.push((samples[2 * i] + samples[2 * i + 1]) * 0.5);
        }
    } else {
        for i in 0..samples.len() {
            mono.push(samples[i]);
        }
    }
    (mono, spec.sample_rate)
}

fn write_audio(path: &str, samples: &[f32], sample_rate: u32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for &sample in samples {
        writer.write_sample(sample).unwrap();
        writer.write_sample(sample).unwrap(); // Duplicate for stereo
    }
}

fn compute_cplx_spectrogram(signal: &[f32]) -> Vec<Vec<Complex<f32>>> {
    let mut engine = StftEngine::new();
    let (frames, n_frames) = engine.forward(signal);
    frames[0..n_frames].to_vec()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut is_semantic = false;
    let mut input_path = String::new();
    let mut output_dir = String::new();
    
    for arg in args.iter().skip(1) {
        if arg == "--semantic" {
            is_semantic = true;
        } else if input_path.is_empty() {
            input_path = arg.clone();
        } else if output_dir.is_empty() {
            output_dir = arg.clone();
        }
    }

    if input_path.is_empty() || output_dir.is_empty() {
        eprintln!("Usage: oracle_extract [--semantic] <input.wav> <output_dir>");
        std::process::exit(1);
    }

    let (signal, actual_sample_rate) = read_audio(&input_path);
    // oracle_extract internally assumes 48000 for its parameters/scout logic,
    // but we write out actual_sample_rate so museval gets correct timing.
    let sample_rate = 48000;

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, sample_rate, None, None, true);

    let k_b = std::env::var("NMFD_K").unwrap_or("8".to_string()).parse::<usize>().unwrap();
    let n_frames = (signal.len() + 512) / 512 + 10; 
    let nmf_b = NmfEngine::new(k_b);
    
    let mut b_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k_b];
    let mut nmfd8_group_masks = vec![vec![0.0_f32; N_BINS]; n_frames];
    
    // For --semantic mode, we need NMF5 masks as well
    let mut nmf5_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; 5];
    let mut nmf5_engine = NmfEngine::new(5);
    nmf5_engine.w = scout.w.clone();
    let mut full_drums = vec![0.0_f32; signal.len()];

    let chunk_len = 65536;
    let history_len = 10240;
    let mut pos = 0;
    let mut total_frames_processed = 0;

    let total_chunks = (signal.len() + chunk_len - 1) / chunk_len;
    let mut chunk_idx = 0;

    while pos < signal.len() {
        let mut hpss_ctx = HpssStreamContext::new();
        chunk_idx += 1;
        eprintln!("Chunk {}/{}", chunk_idx, total_chunks);
        let start = if pos > history_len { pos - history_len } else { 0 };
        let end = std::cmp::min(pos + chunk_len, signal.len());
        let is_last_chunk = pos + chunk_len >= signal.len();
        let chunk_signal = &signal[start..end];
        let chunk_cplx = compute_cplx_spectrogram(chunk_signal);
        let c_frames = chunk_cplx.len();

        let mut chunk_mag = Vec::new();
        let mut chunk_mel = Vec::new();
        for f in &chunk_cplx {
            let mut mag = vec![0.0_f32; 1025];
            for b in 0..1025 {
                mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
            }
            chunk_mag.push(mag.clone());
            let mag_arr: [f32; 1025] = mag.try_into().unwrap();
            chunk_mel.push(sp314_dsp::analysis::mel_128::fold_to_mel(&mag_arr));
        }

        let pad_samples = if pos == 0 { 0 } else { history_len };
        let pad_frames = pad_samples / 512;
        let global_start_frame = (start + pad_samples) / 512;
        
        let core_n_frames = c_frames.saturating_sub(pad_frames);

        // HPSS Time-domain Drums
        let (_mask_h, mask_p) = hpss_ctx.process_chunk(&chunk_mag);
        let core_mask_p = if pad_frames < mask_p.len() { &mask_p[pad_frames..] } else { &[] };
        let core_chunk_signal = if pad_samples < chunk_signal.len() { &chunk_signal[pad_samples..] } else { &[] };
        
        for i in 0..core_chunk_signal.len() {
            if pos + i >= signal.len() { break; }
            let f = i * core_n_frames / core_chunk_signal.len().max(1);
            let w = if f < core_mask_p.len() {
                core_mask_p[f].iter().sum::<f32>() / N_BINS as f32
            } else {
                0.0
            };
            full_drums[pos + i] = core_chunk_signal[i] * w;
        }

        let mut c_v = vec![0.0_f32; 128 * c_frames];
        for f in 0..c_frames {
            for b in 0..128 { c_v[b * c_frames + f] = chunk_mel[f][b]; }
        }
        
        let init_h_chunk = vec![0.1_f32; k_b * c_frames];
        let (chunk_h, _) = nmfd_f32_h_only(&c_v, &scout.tensor_w, &init_h_chunk, 128, k_b, c_frames, scout.tau, 12);

        // Standard NMFD 8 logic (b_masks)
        for comp in 0..k_b {
            let c_mask = nmf_b.nmfd_component_mask_chunk(comp, &chunk_h, &scout.tensor_w, c_frames, N_BINS, scout.tau);
            let core_mask = if c_mask.len() > pad_frames { &c_mask[pad_frames..] } else { &[] };
            
            let mut i = 0;
            while i < core_mask.len() && global_start_frame + i < n_frames {
                b_masks[comp][global_start_frame + i] = core_mask[i].clone();
                if comp == 0 { total_frames_processed = std::cmp::max(total_frames_processed, global_start_frame + i + 1); }
                i += 1;
            }
            if is_last_chunk && !core_mask.is_empty() {
                while global_start_frame + i < n_frames {
                    b_masks[comp][global_start_frame + i] = core_mask.last().unwrap().clone();
                    if comp == 0 { total_frames_processed = std::cmp::max(total_frames_processed, global_start_frame + i + 1); }
                    i += 1;
                }
            }
        }
        
        if is_semantic {
            let group_mask = nmf_b.nmfd_group_mask_chunk(&[0, 1, 2, 3], &chunk_h, &scout.tensor_w, c_frames, N_BINS, scout.tau);
            let core_mask = if group_mask.len() > pad_frames { &group_mask[pad_frames..] } else { &[] };
            let mut i = 0;
            while i < core_mask.len() && global_start_frame + i < n_frames {
                nmfd8_group_masks[global_start_frame + i] = core_mask[i].clone();
                i += 1;
            }
            if is_last_chunk && !core_mask.is_empty() {
                while global_start_frame + i < n_frames {
                    nmfd8_group_masks[global_start_frame + i] = core_mask.last().unwrap().clone();
                    i += 1;
                }
            }
        }

        
        // NMF 5 logic for --semantic mode
        if is_semantic {
            let nmf5_h = nmf5_engine.transform(&scout.w, &chunk_mag);
            for comp in 0..5 {
                let c_mask = nmf5_engine.component_mask_chunk(comp, &nmf5_h, c_frames, N_BINS);
                let core_mask = if c_mask.len() > pad_frames { &c_mask[pad_frames..] } else { &[] };
                
                let mut i = 0;
                while i < core_mask.len() && global_start_frame + i < n_frames {
                    nmf5_masks[comp][global_start_frame + i] = core_mask[i].clone();
                    i += 1;
                }
                if is_last_chunk && !core_mask.is_empty() {
                    while global_start_frame + i < n_frames {
                        nmf5_masks[comp][global_start_frame + i] = core_mask.last().unwrap().clone();
                        i += 1;
                    }
                }
            }
        }

        pos += chunk_len - history_len;
        if is_last_chunk { break; }
    }
    
    let full_cplx = compute_cplx_spectrogram(&signal);
    
    if is_semantic {
        // Semantic mode: output to nmf5/ and nmfd8/ in output_dir
        let nmf5_dir = format!("{}/nmf5", output_dir);
        let nmfd8_dir = format!("{}/nmfd8", output_dir);
        std::fs::create_dir_all(&nmf5_dir).unwrap();
        std::fs::create_dir_all(&nmfd8_dir).unwrap();

        // Write Drums (Byte-Identical)
        write_audio(&format!("{}/drums.wav", nmf5_dir), &full_drums, actual_sample_rate);
        write_audio(&format!("{}/drums.wav", nmfd8_dir), &full_drums, actual_sample_rate);
        
        // ----- NMF5 Render -----
        let mut nmf5_vocals_cplx = full_cplx.clone();
        let mut nmf5_bass_cplx = full_cplx.clone();
        let mut nmf5_harm_cplx = full_cplx.clone();
        let mut nmf5_amb_cplx = full_cplx.clone();
        
        for f in 0..std::cmp::min(full_cplx.len(), total_frames_processed) {
            for b in 0..N_BINS {
                let v_m = nmf5_masks[scout.voice_idx][f][b];
                nmf5_vocals_cplx[f][b].re *= v_m; nmf5_vocals_cplx[f][b].im *= v_m;
                
                let b_m = nmf5_masks[scout.bass_idx][f][b];
                nmf5_bass_cplx[f][b].re *= b_m; nmf5_bass_cplx[f][b].im *= b_m;
                
                let h_m = nmf5_masks[scout.harmonics_idx][f][b];
                nmf5_harm_cplx[f][b].re *= h_m; nmf5_harm_cplx[f][b].im *= h_m;
                
                let a_m = nmf5_masks[scout.ambience_idx][f][b];
                nmf5_amb_cplx[f][b].re *= a_m; nmf5_amb_cplx[f][b].im *= a_m;
            }
        }
        
        let mut stft = StftEngine::new();
        let out_v = stft.inverse(&nmf5_vocals_cplx, signal.len());
        write_audio(&format!("{}/vocals.wav", nmf5_dir), &out_v, actual_sample_rate);
        
        let mut stft = StftEngine::new();
        let out_b = stft.inverse(&nmf5_bass_cplx, signal.len());
        write_audio(&format!("{}/bass.wav", nmf5_dir), &out_b, actual_sample_rate);
        
        let mut stft = StftEngine::new();
        let out_h = stft.inverse(&nmf5_harm_cplx, signal.len());
        let mut stft = StftEngine::new();
        let out_a = stft.inverse(&nmf5_amb_cplx, signal.len());
        let out_other: Vec<f32> = out_h.iter().zip(out_a.iter()).map(|(h, a)| h + a).collect();
        write_audio(&format!("{}/harmonics.wav", nmf5_dir), &out_h, actual_sample_rate);
        write_audio(&format!("{}/ambience.wav", nmf5_dir), &out_a, actual_sample_rate);
        write_audio(&format!("{}/other.wav", nmf5_dir), &out_other, actual_sample_rate);
        
        // ----- NMFD8 Render -----
        let mut nmfd8_vocals_cplx = full_cplx.clone();
        let mut nmfd8_bass_cplx = full_cplx.clone();
        let mut nmfd8_other_cplx = full_cplx.clone();
        let mut nmfd8_harm_cplx = full_cplx.clone();
        let mut nmfd8_amb_cplx = full_cplx.clone();
        
        for f in 0..std::cmp::min(full_cplx.len(), total_frames_processed) {
            for b in 0..N_BINS {
                // vocals: we stored the nmfd_group_mask_chunk result in nmfd8_group_masks
                let mut v_m = nmfd8_group_masks[f][b];
                // clamp mask just in case
                v_m = v_m.min(1.0);
                nmfd8_vocals_cplx[f][b].re *= v_m; nmfd8_vocals_cplx[f][b].im *= v_m;
                
                // bass
                let b_m = b_masks[scout.nmfd_bass_idx][f][b];
                nmfd8_bass_cplx[f][b].re *= b_m; nmfd8_bass_cplx[f][b].im *= b_m;
                
                // other (harmonics + ambience)
                let o_m = (b_masks[scout.nmfd_harmonics_idx][f][b] + b_masks[scout.nmfd_ambience_idx][f][b]).min(1.0);
                nmfd8_other_cplx[f][b].re *= o_m; nmfd8_other_cplx[f][b].im *= o_m;
                
                let h_m = b_masks[scout.nmfd_harmonics_idx][f][b].min(1.0);
                nmfd8_harm_cplx[f][b].re *= h_m;
                nmfd8_harm_cplx[f][b].im *= h_m;
                
                let a_m = b_masks[scout.nmfd_ambience_idx][f][b].min(1.0);
                nmfd8_amb_cplx[f][b].re *= a_m;
                nmfd8_amb_cplx[f][b].im *= a_m;
            }
        }
        
        let mut stft = StftEngine::new();
        let out_v = stft.inverse(&nmfd8_vocals_cplx, signal.len());
        write_audio(&format!("{}/vocals.wav", nmfd8_dir), &out_v, actual_sample_rate);
        
        let mut stft = StftEngine::new();
        let out_b = stft.inverse(&nmfd8_bass_cplx, signal.len());
        write_audio(&format!("{}/bass.wav", nmfd8_dir), &out_b, actual_sample_rate);
        
        let mut stft = StftEngine::new();
        let out_o = stft.inverse(&nmfd8_other_cplx, signal.len());
        write_audio(&format!("{}/other.wav", nmfd8_dir), &out_o, actual_sample_rate);
        
        let mut stft = StftEngine::new();
        let out_h = stft.inverse(&nmfd8_harm_cplx, signal.len());
        write_audio(&format!("{}/harmonics.wav", nmfd8_dir), &out_h, actual_sample_rate);
        
        let mut stft = StftEngine::new();
        let out_a = stft.inverse(&nmfd8_amb_cplx, signal.len());
        write_audio(&format!("{}/ambience.wav", nmfd8_dir), &out_a, actual_sample_rate);

    } else {
        // Original behavior
        for comp in 0..k_b {
            let mut stft = StftEngine::new();
            let mut comp_cplx = full_cplx.clone();
            for f in 0..std::cmp::min(comp_cplx.len(), total_frames_processed) {
                for b in 0..N_BINS {
                    let mask = b_masks[comp][f][b];
                    comp_cplx[f][b].re *= mask;
                    comp_cplx[f][b].im *= mask;
                }
            }
            
            let out_signal = stft.inverse(&comp_cplx, signal.len());
            let out_path = format!("{}/comp{}.wav", output_dir, comp);
            write_audio(&out_path, &out_signal, actual_sample_rate);
        }
    }
}
