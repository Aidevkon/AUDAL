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

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: phi1_mel <input.wav> <out.csv>");
        std::process::exit(1);
    }
    let input_path = &args[1];
    let output_path = &args[2];

    let mut reader = WavReader::open(input_path).unwrap();
    let spec = reader.spec();
    if spec.sample_rate != 16000 || spec.channels != 1 {
        eprintln!("Error: Audio must be 16k mono");
        std::process::exit(1);
    }
    
    let mut x: Vec<f32> = Vec::new();
    if spec.sample_format == hound::SampleFormat::Float {
        x = reader.samples::<f32>().map(|s| s.unwrap()).collect();
    } else {
        eprintln!("Error: Audio must be float32");
        std::process::exit(1);
    }

    let n = x.len();
    let pad = 200;
    let padded_len = pad + n + pad;
    let mut padded = vec![0.0f32; padded_len];
    
    padded[pad..pad + n].copy_from_slice(&x);
    // reflect ΧΩΡΙΣ επανάληψη του άκρου:
    // x[0], x[1], x[2]... -> αριστερά padding: x[1], x[2]...
    // Έτσι αν έχω 200 padding, padded[199-i] = x[i+1]
    for i in 0..pad {
        if i + 1 < n {
            padded[pad - 1 - i] = x[i + 1];
        }
    }
    // δεξιά padding: 
    // x[N-2], x[N-3]...
    for i in 0..pad {
        if n >= 2 + i {
            padded[pad + n + i] = x[n - 2 - i];
        }
    }

    let n_fft = 400;
    let hop = 160;
    let n_mels = 64;
    let sample_rate = 16000.0;
    
    let mut w = vec![0.0f32; n_fft];
    for i in 0..n_fft {
        w[i] = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n_fft as f32).cos();
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n_fft);

    // Filterbank setup
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

    let out_file = File::create(output_path).unwrap();
    let mut writer = BufWriter::new(out_file);

    let mut k = 0;
    loop {
        let start = k * hop;
        if start + n_fft > padded_len {
            break;
        }

        let frame = &padded[start..start + n_fft];
        let mut buffer: Vec<Complex<f32>> = frame.iter().zip(w.iter())
            .map(|(&x, &win)| Complex { re: x * win, im: 0.0 })
            .collect();
        
        fft.process(&mut buffer);

        let mut power = vec![0.0f32; n_freqs];
        for i in 0..n_freqs {
            let re = buffer[i].re;
            let im = buffer[i].im;
            power[i] = re * re + im * im;
        }

        write!(writer, "{}", k).unwrap();
        for m in 0..n_mels {
            let mut energy = 0.0f32;
            for f in 0..n_freqs {
                energy += fb[f][m] * power[f];
            }
            let log_energy = (energy + 1e-9).ln();
            write!(writer, ",{}", log_energy).unwrap();
        }
        writeln!(writer).unwrap();

        k += 1;
    }
}
