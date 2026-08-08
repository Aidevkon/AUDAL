use hound::WavReader;
use rustfft::{num_complex::Complex, FftPlanner};
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10_f32.powf(mel / 2595.0) - 1.0)
}

fn read_f32_le(data: &[u8], offset: &mut usize) -> f32 {
    let bytes = [
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ];
    *offset += 4;
    f32::from_le_bytes(bytes)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: mvad_phi1 <input.wav> [window_start_s window_end_s] [--dump=<csv>] [--norm=<chunk|global|warmup>]");
        std::process::exit(1);
    }
    let input_path = &args[1];
    let mut window_start_s = -1.0f32;
    let mut window_end_s = -1.0f32;
    let mut dump_csv: Option<String> = None;
    let mut norm_mode = "chunk".to_string();
    let mut weights_path: Option<String> = None;
    let mut frontend = "logmel".to_string();

    for arg in args.iter().skip(2) {
        if arg.starts_with("--dump=") {
            dump_csv = Some(arg[7..].to_string());
        } else if arg.starts_with("--norm=") {
            norm_mode = arg[7..].to_string();
        } else if arg.starts_with("--weights=") {
            weights_path = Some(arg[10..].to_string());
        } else if arg.starts_with("--frontend=") {
            frontend = arg[11..].to_string();
        } else if window_start_s < 0.0 {
            window_start_s = arg.parse().unwrap();
        } else if window_end_s < 0.0 {
            window_end_s = arg.parse().unwrap();
        }
    }

    let embedded_bytes = include_bytes!("../../../assets/phi1_v3.bin").to_vec();
    let model_bytes = if let Some(path) = &weights_path {
        std::fs::read(path).expect("Failed to read weights file")
    } else {
        embedded_bytes
    };
    if model_bytes.len() != 243332 {
        panic!("Invalid model size: {} bytes", model_bytes.len());
    }

    let mut offset = 0;
    
    // conv1: 48, 64, 11
    let mut conv1_w = vec![vec![vec![0.0f32; 11]; 64]; 48];
    for o in 0..48 {
        for c in 0..64 {
            for k in 0..11 {
                conv1_w[o][c][k] = read_f32_le(&model_bytes, &mut offset);
            }
        }
    }
    let mut conv1_b = vec![0.0f32; 48];
    for o in 0..48 {
        conv1_b[o] = read_f32_le(&model_bytes, &mut offset);
    }

    // conv2: 48, 48, 11
    let mut conv2_w = vec![vec![vec![0.0f32; 11]; 48]; 48];
    for o in 0..48 {
        for c in 0..48 {
            for k in 0..11 {
                conv2_w[o][c][k] = read_f32_le(&model_bytes, &mut offset);
            }
        }
    }
    let mut conv2_b = vec![0.0f32; 48];
    for o in 0..48 {
        conv2_b[o] = read_f32_le(&model_bytes, &mut offset);
    }

    // fc1: 32, 48
    let mut fc1_w = vec![vec![0.0f32; 48]; 32];
    for o in 0..32 {
        for c in 0..48 {
            fc1_w[o][c] = read_f32_le(&model_bytes, &mut offset);
        }
    }
    let mut fc1_b = vec![0.0f32; 32];
    for o in 0..32 {
        fc1_b[o] = read_f32_le(&model_bytes, &mut offset);
    }

    // fc2: 1, 32
    let mut fc2_w = vec![vec![0.0f32; 32]; 1];
    for c in 0..32 {
        fc2_w[0][c] = read_f32_le(&model_bytes, &mut offset);
    }
    let fc2_b = read_f32_le(&model_bytes, &mut offset);

    // Read audio
    let mut reader = WavReader::open(input_path).unwrap();
    let spec = reader.spec();
    let num_channels = spec.channels as usize;
    
    // Downmix to mono and store float
    let mut mono: Vec<f32> = Vec::new();
    if spec.sample_format == hound::SampleFormat::Float {
        let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
        for chunk in samples.chunks(num_channels) {
            let mut sum = 0.0;
            for &s in chunk {
                sum += s;
            }
            mono.push(sum / num_channels as f32);
        }
    } else {
        let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        for chunk in samples.chunks(num_channels) {
            let mut sum = 0.0;
            for &s in chunk {
                sum += s as f32 / 32768.0;
            }
            mono.push(sum / num_channels as f32);
        }
    }

    // Resample to 16k
    let orig_sr = spec.sample_rate as f32;
    let mut x: Vec<f32> = Vec::new();
    
    if orig_sr == 16000.0 {
        x = mono;
    } else {
        let mut filtered = mono;
        if orig_sr > 16000.0 {
            let fc = 7600.0 / orig_sr;
            let m = 120;
            let mut h = vec![0.0f32; m + 1];
            let mut sum_h = 0.0;
            for n in 0..=m {
                let n_f32 = n as f32;
                let m_f32 = m as f32;
                let hamming = 0.54 - 0.46 * (2.0 * std::f32::consts::PI * n_f32 / m_f32).cos();
                let x_val = n_f32 - m_f32 / 2.0;
                let sinc = if x_val.abs() < 1e-6 {
                    2.0 * fc
                } else {
                    (2.0 * std::f32::consts::PI * fc * x_val).sin() / (std::f32::consts::PI * x_val)
                };
                h[n] = sinc * hamming;
                sum_h += h[n];
            }
            for n in 0..=m {
                h[n] /= sum_h;
            }
            
            let len = filtered.len();
            let mut conv_out = vec![0.0f32; len];
            let half_m = m / 2; // 60
            for i in 0..len {
                let mut acc = 0.0;
                for j in 0..=m {
                    let mut src_idx = i as isize + j as isize - half_m as isize;
                    if src_idx < 0 { src_idx = 0; }
                    if src_idx >= len as isize { src_idx = len as isize - 1; }
                    acc += h[j] * filtered[src_idx as usize];
                }
                conv_out[i] = acc;
            }
            filtered = conv_out;
        }

        if orig_sr == 48000.0 {
            x = filtered.into_iter().step_by(3).collect();
        } else {
            let ratio = orig_sr / 16000.0;
            let new_len = (filtered.len() as f32 / ratio).ceil() as usize;
            for i in 0..new_len {
                let src_idx = (i as f32) * ratio;
                let idx1 = src_idx.floor() as usize;
                let idx2 = (idx1 + 1).min(filtered.len() - 1);
                let frac = src_idx - idx1 as f32;
                let val = filtered[idx1] * (1.0 - frac) + filtered[idx2] * frac;
                x.push(val);
            }
        }
    }


    let mut norm_mode = "chunk".to_string();
    for arg in args.iter().skip(2) {
        if arg.starts_with("--dump=") {
            dump_csv = Some(arg[7..].to_string());
        } else if arg.starts_with("--norm=") {
            norm_mode = arg[7..].to_string();
        } else if arg.starts_with("--weights=") {
            weights_path = Some(arg[10..].to_string());
        } else if arg.starts_with("--frontend=") {
            frontend = arg[11..].to_string();
        } else if window_start_s < 0.0 {
            window_start_s = arg.parse().unwrap();
        } else if window_end_s < 0.0 {
            window_end_s = arg.parse().unwrap();
        }
    }

    let chunk_size = if norm_mode == "rolling" { x.len().max(1) } else { 16000 * 5 };

    let n_fft = 400;
    let hop = 160;
    let pad = 200;
    let n_mels = 64;
    let sample_rate = 16000.0;

    let mut w = vec![0.0f32; n_fft];
    for i in 0..n_fft {
        w[i] = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n_fft as f32).cos();
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n_fft);

    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(8000.0);
    let mut mel_points = vec![0.0f32; n_mels + 2];
    for i in 0..(n_mels + 2) {
        mel_points[i] = mel_min + (i as f32) * (mel_max - mel_min) / (n_mels as f32 + 1.0);
    }
    let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

    let n_freqs = n_fft / 2 + 1; // 201
    let mut fb = vec![vec![0.0f32; n_mels]; n_freqs];
    for f in 0..n_freqs {
        let freq = (f as f32) * sample_rate / n_fft as f32;
        for m in 0..n_mels {
            let left = hz_points[m];
            let center = hz_points[m + 1];
            let right = hz_points[m + 2];

            if freq > left && freq < center {
                fb[f][m] = (freq - left) / (center - left);
            } else if freq >= center && freq < right {
                fb[f][m] = (right - freq) / (right - center);
            }
        }
    }

    let mut dump_file = None;
    if let Some(path) = &dump_csv {
        dump_file = Some(BufWriter::new(File::create(path).unwrap()));
    }

    let mut all_p = Vec::new();
    let mut all_t = Vec::new();

    let mut all_chunks_mels = Vec::new();
    let mut start_idx = 0;
    while start_idx < x.len() {
        let end_idx = (start_idx + chunk_size).min(x.len());
        let chunk = &x[start_idx..end_idx];
        if chunk.len() < 16000 {
            break; // Skip chunks < 1s (like Python)
        }

        let n = chunk.len();
        let padded_len = pad + n + pad;
        let mut padded = vec![0.0f32; padded_len];
        padded[pad..pad + n].copy_from_slice(chunk);
        for i in 0..pad {
            if i + 1 < n {
                padded[pad - 1 - i] = chunk[i + 1];
            }
            if n >= 2 + i {
                padded[pad + n + i] = chunk[n - 2 - i];
            }
        }

        let mut mel_matrix = Vec::new();
        let mut k = 0;
        loop {
            let start = k * hop;
            if start + n_fft > padded_len {
                break;
            }

            let frame = &padded[start..start + n_fft];
            let mut buffer: Vec<Complex<f32>> = frame.iter().zip(w.iter())
                .map(|(&xv, &win)| Complex { re: xv * win, im: 0.0 })
                .collect();
            
            fft.process(&mut buffer);

            let mut power = vec![0.0f32; n_freqs];
            for i in 0..n_freqs {
                power[i] = buffer[i].re * buffer[i].re + buffer[i].im * buffer[i].im;
            }

            let mut mel_frame = vec![0.0f32; n_mels];
            for m in 0..n_mels {
                let mut energy = 0.0f32;
                for f in 0..n_freqs {
                    energy += fb[f][m] * power[f];
                }
                if frontend == "logmel" {
                    mel_frame[m] = (energy + 1e-9).ln();
                } else {
                    mel_frame[m] = energy;
                }
            }
            mel_matrix.push(mel_frame);
            k += 1;
        }

        all_chunks_mels.push(mel_matrix);
        start_idx += chunk_size;
    }

    let mut chunk_idx = 0;
    let mut saved_mean = 0.0;
    let mut saved_std = 1.0;
    
    let mut global_mean = 0.0;
    let mut global_std = 1.0;
    if norm_mode == "global" || norm_mode == "scout30mid" || norm_mode == "scout30head" {
        let n_frames: usize = all_chunks_mels.iter().map(|c| c.len()).sum();
        
        let (start_frame, end_frame) = if norm_mode == "global" || n_frames <= 3000 {
            (0, n_frames)
        } else if norm_mode == "scout30mid" {
            let s = (n_frames - 3000) / 2;
            (s, s + 3000)
        } else {
            (0, 3000)
        };

        let mut sum = 0.0;
        let mut count = 0;
        let mut current_frame = 0;
        
        for chunk_mels in &all_chunks_mels {
            for frame in chunk_mels {
                if current_frame >= start_frame && current_frame < end_frame {
                    for &val in frame { sum += val; count += 1; }
                }
                current_frame += 1;
            }
        }
        global_mean = if count > 0 { sum / count as f32 } else { 0.0 };
        
        let mut sum_sq = 0.0;
        current_frame = 0;
        for chunk_mels in &all_chunks_mels {
            for frame in chunk_mels {
                if current_frame >= start_frame && current_frame < end_frame {
                    for &val in frame { let d = val - global_mean; sum_sq += d * d; }
                }
                current_frame += 1;
            }
        }
        global_std = if count > 1 { (sum_sq / (count as f32 - 1.0)).sqrt() } else { 1.0 };
    }


    start_idx = 0;
    let mut prev_mels: Vec<Vec<f32>> = Vec::new();
    let mut pcen_m = vec![0.0f32; 64];
    let mut pcen_initialized = false;
    for mut mel_matrix in all_chunks_mels {
        let num_frames = mel_matrix.len();
        if num_frames == 0 {
            start_idx += chunk_size;
            chunk_idx += 1;
            continue;
        }

        let mut mean = 0.0;
        let mut std = 1.0;


        if frontend == "pcen" {
            for frame in &mut mel_matrix {
                if !pcen_initialized {
                    for m in 0..64 {
                        pcen_m[m] = frame[m];
                    }
                    pcen_initialized = true;
                }
                for m in 0..64 {
                    pcen_m[m] = (1.0 - 0.025) * pcen_m[m] + 0.025 * frame[m];
                    let out = (frame[m] / (1e-6 + pcen_m[m]).powf(0.98) + 2.0).powf(0.5) - 2.0f32.powf(0.5);
                    frame[m] = out;
                }
            }
        } else {
            if norm_mode == "chunk" || norm_mode == "chunkcont" || (norm_mode == "warmup" && chunk_idx == 0) {
                let mut sum = 0.0;
                let mut count = 0;
                for frame in &mel_matrix {
                    for &val in frame { sum += val; count += 1; }
                }
                mean = sum / count as f32;
                let mut sum_sq = 0.0;
                for frame in &mel_matrix {
                    for &val in frame { let d = val - mean; sum_sq += d * d; }
                }
                std = (sum_sq / (count as f32 - 1.0)).sqrt();

                if norm_mode == "warmup" && chunk_idx == 0 {
                    saved_mean = mean;
                    saved_std = std;
                }
            } else if norm_mode == "warmup" {
                mean = saved_mean;
                std = saved_std;
            } else if norm_mode == "global" {
                mean = global_mean;
                std = global_std;
            }

            for frame in &mut mel_matrix {
                for val in frame.iter_mut() {
                    *val = (*val - mean) / (std + 1e-5);
                }
            }
        }

        // Inference (from frame 50 onwards)
        let chunk_start_s = start_idx as f32 / 16000.0;
        let inference_start = if (norm_mode == "chunkcont" || frontend == "pcen") && chunk_idx > 0 { 0 } else { 50 };
        for i in inference_start..num_frames {
            // window = 64 x 51
            // mel_matrix shape: [num_frames][64]
            let mut window = vec![vec![0.0f32; 51]; 64];
            for t in 0..51 {
                let frame_idx = i as isize - 50 + t as isize;
                for c in 0..64 {
                    if frame_idx < 0 {
                        let prev_i = prev_mels.len() as isize + frame_idx;
                        if prev_i >= 0 {
                            window[c][t] = prev_mels[prev_i as usize][c];
                        } else {
                            window[c][t] = 0.0;
                        }
                    } else {
                        window[c][t] = mel_matrix[frame_idx as usize][c];
                    }
                }
            }

            // conv1: in=64, out=48, kernel=11, d=1
            let mut y1 = vec![vec![0.0f32; 41]; 48];
            for o in 0..48 {
                for t in 0..41 {
                    let mut sum_val = conv1_b[o];
                    for c in 0..64 {
                        for k in 0..11 {
                            sum_val += conv1_w[o][c][k] * window[c][t + k];
                        }
                    }
                    y1[o][t] = sum_val.tanh();
                }
            }

            // conv2: in=48, out=48, kernel=11, d=4, out_time=1
            let mut y2 = vec![0.0f32; 48];
            for o in 0..48 {
                let mut sum_val = conv2_b[o];
                for c in 0..48 {
                    for k in 0..11 {
                        sum_val += conv2_w[o][c][k] * y1[c][k * 4];
                    }
                }
                y2[o] = sum_val.tanh();
            }

            // fc1
            let mut fc1_out = vec![0.0f32; 32];
            for o in 0..32 {
                let mut sum_val = fc1_b[o];
                for c in 0..48 {
                    sum_val += fc1_w[o][c] * y2[c];
                }
                fc1_out[o] = sum_val.tanh();
            }

            // fc2
            let mut sum_val = fc2_b;
            for c in 0..32 {
                sum_val += fc2_w[0][c] * fc1_out[c];
            }
            
            // sigmoid
            let p = 1.0 / (1.0 + (-sum_val).exp());
            
            let frame_time = chunk_start_s + (i as f32) * 0.01;
            all_t.push(frame_time);
            all_p.push(p);

            if let Some(w) = &mut dump_file {
                writeln!(w, "{:.6},{:.6}", frame_time, p).unwrap();
            }
        }
        if norm_mode == "chunkcont" || frontend == "pcen" {
            let take = 50.min(mel_matrix.len());
            prev_mels = mel_matrix[mel_matrix.len() - take..].to_vec();
        }

        start_idx += chunk_size;
        chunk_idx += 1;
    }

    if window_start_s < 0.0 { window_start_s = 0.0; }
    if window_end_s < 0.0 { window_end_s = f32::INFINITY; }

    let mut in_05 = 0;
    let mut in_07 = 0;
    let mut in_total = 0;
    
    let mut out_05 = 0;
    let mut out_07 = 0;
    let mut out_total = 0;

    for (&t, &p) in all_t.iter().zip(all_p.iter()) {
        if t >= window_start_s && t <= window_end_s {
            in_total += 1;
            if p > 0.5 { in_05 += 1; }
            if p > 0.7 { in_07 += 1; }
        } else {
            out_total += 1;
            if p > 0.5 { out_05 += 1; }
            if p > 0.7 { out_07 += 1; }
        }
    }

    let in_05_rat = if in_total > 0 { in_05 as f32 / in_total as f32 } else { 0.0 };
    let in_07_rat = if in_total > 0 { in_07 as f32 / in_total as f32 } else { 0.0 };
    let out_05_rat = if out_total > 0 { out_05 as f32 / out_total as f32 } else { 0.0 };
    let out_07_rat = if out_total > 0 { out_07 as f32 / out_total as f32 } else { 0.0 };

    let total_frames = all_p.len();
    let total_05 = in_05 + out_05;
    let total_07 = in_07 + out_07;
    let ratio_05 = if total_frames > 0 { total_05 as f32 / total_frames as f32 } else { 0.0 };
    let ratio_07 = if total_frames > 0 { total_07 as f32 / total_frames as f32 } else { 0.0 };

    let file_name = std::path::Path::new(input_path).file_name().unwrap().to_str().unwrap();
    println!("PHI1|file={}|frames={}|ratio_05={:.3}|ratio_07={:.3}|in_05={:.3}|in_07={:.3}|out_05={:.3}|out_07={:.3}",
        file_name, total_frames, ratio_05, ratio_07, in_05_rat, in_07_rat, out_05_rat, out_07_rat);
}
