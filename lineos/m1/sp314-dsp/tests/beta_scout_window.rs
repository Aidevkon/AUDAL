use hound::WavReader;
use std::path::Path;
use sp314_dsp::analysis::phi1_sensor::{Phi2StreamingFrontend, Phi2Pcen, Phi2Sensor};
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::nmf::find_most_diverse_window;
use sp314_dsp::stft::{StftEngine, N_BINS};

#[ignore]
#[test]
fn test_beta_scout_window() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    if !input_path.exists() {
        println!("SKIPPED: {:?} missing", input_path);
        return;
    }

    let mut reader = WavReader::open(input_path).unwrap();
    let spec = reader.spec();
    let mut mono = Vec::new();
    if spec.sample_format == hound::SampleFormat::Float {
        let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
        if spec.channels == 2 {
            for i in 0..(samples.len() / 2) {
                mono.push((samples[2 * i] + samples[2 * i + 1]) * 0.5);
            }
        } else {
            mono = samples;
        }
    } else {
        let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        if spec.channels == 2 {
            for i in 0..(samples.len() / 2) {
                mono.push((samples[2 * i] as f32 + samples[2 * i + 1] as f32) * 0.5 / 32768.0);
            }
        } else {
            mono = samples.into_iter().map(|s| s as f32 / 32768.0).collect();
        }
    }
    let sample_rate = spec.sample_rate;

    let mut fe = Phi2StreamingFrontend::new();
    let mut pcen = Phi2Pcen::new();
    let mut sensor = Phi2Sensor::new();

    let frames1 = fe.push(&mono);
    let frames2 = fe.finish();

    let mut p_vals = Vec::new();
    for mel in frames1.iter().chain(frames2.iter()) {
        let pcen_frame = pcen.process(mel);
        if let Some(p) = sensor.push_frame(&pcen_frame) {
            p_vals.push(p);
        }
    }

    let window_30s = 30 * sample_rate as usize;
    let window_frames = 30 * sample_rate as usize / 480;

    let a_start = 0;
    let a_end = window_30s.min(mono.len());
    let a_slice = &mono[a_start..a_end];

    let (b_start, b_end) = find_most_diverse_window(&mono, sample_rate, 30.0);
    let b_slice = &mono[b_start..b_end];

    let mut max_sum = 0.0f32;
    let mut best_start_frame = 0;
    if p_vals.len() > window_frames {
        let mut current_sum: f32 = p_vals.iter().take(window_frames).sum();
        max_sum = current_sum;
        for i in 1..=(p_vals.len() - window_frames) {
            current_sum = current_sum - p_vals[i - 1] + p_vals[i + window_frames - 1];
            if current_sum > max_sum {
                max_sum = current_sum;
                best_start_frame = i;
            }
        }
    }
    let c_start = best_start_frame * 480;
    let c_end = (c_start + window_30s).min(mono.len());
    let c_slice = &mono[c_start..c_end];

    let test_cases = [
        ("A_first30", a_slice),
        ("B_diverse", b_slice),
        ("C_speech", c_slice),
        ("D_fullscout", mono.as_slice()),
    ];

    println!("{:<15} | {:<9} | {:>6} | {:>7} | {:>6} | {:>6} | {:>6} | {:>9}",
        "scenario", "stem", "<150", "150-500", "500-2k", "2k-6k", ">6k", "rms");
    println!("{}", "-".repeat(80));

    for (name, scout_slice) in &test_cases {
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(scout_slice, sample_rate, None, None, true);
        
        let mut voice_stem = Vec::new();
        let mut harm_stem = Vec::new();
        engine.process_chunks(&mono, &scout, true, |chunk| {
            voice_stem.extend_from_slice(&chunk.voice);
            harm_stem.extend_from_slice(&chunk.harmonics);
        }).unwrap();

        for (stem_name, stem) in [("voice", voice_stem), ("harmonics", harm_stem)] {
            let mut stft = StftEngine::new();
            let (cplx, n_frames) = stft.forward(&stem);
            
            let mut energies = [0.0f64; 5];
            for f in 0..n_frames {
                for b in 0..N_BINS {
                    let freq = b as f32 * sample_rate as f32 / 2048.0;
                    let cplx_val = cplx[f][b];
                    let mag_sq = (cplx_val.re * cplx_val.re + cplx_val.im * cplx_val.im) as f64;

                    if freq < 150.0 {
                        energies[0] += mag_sq;
                    } else if freq < 500.0 {
                        energies[1] += mag_sq;
                    } else if freq < 2000.0 {
                        energies[2] += mag_sq;
                    } else if freq < 6000.0 {
                        energies[3] += mag_sq;
                    } else {
                        energies[4] += mag_sq;
                    }
                }
            }
            
            let total_e: f64 = energies.iter().sum();
            let pcts: Vec<f64> = if total_e > 1e-12 {
                energies.iter().map(|&e| e / total_e * 100.0).collect()
            } else {
                vec![0.0; 5]
            };
            
            let sum_sq: f32 = stem.iter().map(|&x| x * x).sum();
            let rms = (sum_sq / stem.len() as f32).sqrt();

            println!("{:<15} | {:<9} | {:>5.1}% | {:>6.1}% | {:>5.1}% | {:>5.1}% | {:>5.1}% | {:>9.6}",
                name, stem_name, pcts[0], pcts[1], pcts[2], pcts[3], pcts[4], rms);
        }
        println!("{}", "-".repeat(80));
    }
}
