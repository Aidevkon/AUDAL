use hound;
use rustfft::num_complex::Complex;
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::{StftEngine, N_BINS};
use std::env;

fn read_audio(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = reader.samples().map(|s| s.unwrap()).collect();
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
    mono
}

fn write_audio(path: &str, samples: &[f32], sample_rate: u32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for &sample in samples {
        writer.write_sample(sample).unwrap();
    }
}

fn compute_cplx_spectrogram(signal: &[f32]) -> Vec<Vec<Complex<f32>>> {
    let mut engine = StftEngine::new();
    let (frames, n_frames) = engine.forward(signal);
    frames[0..n_frames].to_vec()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: oracle_extract <input.wav> <output_dir>");
        std::process::exit(1);
    }
    let input_path = &args[1];
    let output_dir = &args[2];

    let signal = read_audio(input_path);
    let sample_rate = 48000;

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, sample_rate, None, None);

    let k_b = std::env::var("NMFD_K").unwrap_or("8".to_string()).parse::<usize>().unwrap();
    let n_frames = (signal.len() + 512) / 512 + 10; 
    let nmf_b = NmfEngine::new(k_b);
    let mut b_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k_b];

    let chunk_len = 65536;
    let history_len = 10240;
    let mut pos = 0;
    let mut total_frames_processed = 0;

    let total_chunks = (signal.len() + chunk_len - 1) / chunk_len;
    let mut chunk_idx = 0;

    while pos < signal.len() {
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
                let src_b = (b * (N_BINS - 1)) / 1024;
                mag[b] = libm::sqrtf(f[src_b].re * f[src_b].re + f[src_b].im * f[src_b].im);
            }
            chunk_mag.push(mag.clone());
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
        let (chunk_h, _) = nmfd_f32_h_only(&c_v, &scout.tensor_w, &init_h_chunk, 128, k_b, c_frames, scout.tau, 12);

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

        pos += chunk_len - history_len;
        if is_last_chunk { break; }
    }
    
    let full_cplx = compute_cplx_spectrogram(&signal);
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
        write_audio(&out_path, &out_signal, sample_rate as u32);
    }
}
