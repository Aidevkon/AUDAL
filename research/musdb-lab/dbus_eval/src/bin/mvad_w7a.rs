use hound;
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{VadClassifier, FixedPriors};
use std::env;

fn load_wav(path: &str) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).expect("Failed to open wav file");
    let channels = reader.spec().channels;
    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
    
    let mut mono = Vec::new();
    let mut left = Vec::new();
    let mut right = Vec::new();
    
    if channels == 2 {
        for chunk in samples.chunks_exact(2) {
            left.push(chunk[0]);
            right.push(chunk[1]);
            mono.push((chunk[0] + chunk[1]) * 0.5);
        }
    } else {
        for s in samples {
            left.push(s);
            right.push(s);
            mono.push(s);
        }
    }
    
    (mono, left, right)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input.wav> [window_start_s window_end_s]", args[0]);
        std::process::exit(1);
    }
    
    let path = &args[1];
    let mut has_window = false;
    let mut start_idx = 0;
    let mut end_idx = 0;
    
    if args.len() >= 4 {
        has_window = true;
        let start_s: f32 = args[2].parse().expect("Invalid start window");
        let end_s: f32 = args[3].parse().expect("Invalid end window");
        start_idx = (start_s * 100.0).floor() as usize;
        end_idx = (end_s * 100.0).floor() as usize;
    }
    
    let (mono, left, right) = load_wav(path);
    
    // 1. Noise Floor (as required by VadContext)
    let mut acx = AcxCheckAnalyzer::new(48000);
    for chunk in mono.chunks(1777) {
        acx.feed_chunk(chunk);
    }
    let noise_floor = acx.finish().noise_floor_db.unwrap_or(-70.0);
    
    // 2. Feature Extractor
    let mut extractor = VadFeatureExtractor::new();
    let features = extractor.process_chunk(&mono, &left, &right);
    
    // 3. Classifier
    let mut classifier = VadClassifier::new(FixedPriors);
    
    let mut speech_frames_all = 0;
    let mut sum_p_all = 0.0;
    let mut total_frames_all = 0;
    
    let mut speech_frames_in = 0;
    let mut total_frames_in = 0;
    
    let mut speech_frames_out = 0;
    let mut total_frames_out = 0;
    
    for (i, f) in features.iter().enumerate() {
        let d = classifier.process(f, noise_floor);
        
        total_frames_all += 1;
        sum_p_all += d.posterior;
        if d.is_speech {
            speech_frames_all += 1;
        }
        
        if has_window {
            if i >= start_idx && i <= end_idx {
                total_frames_in += 1;
                if d.is_speech {
                    speech_frames_in += 1;
                }
            } else {
                total_frames_out += 1;
                if d.is_speech {
                    speech_frames_out += 1;
                }
            }
        }
    }
    
    let ratio_all = if total_frames_all == 0 { 0.0 } else { speech_frames_all as f64 / total_frames_all as f64 };
    let p_mean_all = if total_frames_all == 0 { 0.0 } else { sum_p_all as f64 / total_frames_all as f64 };
    
    let mut ratio_in = -1.0;
    let mut ratio_out = -1.0;
    
    if has_window {
        ratio_in = if total_frames_in == 0 { 0.0 } else { speech_frames_in as f64 / total_frames_in as f64 };
        ratio_out = if total_frames_out == 0 { 0.0 } else { speech_frames_out as f64 / total_frames_out as f64 };
    }
    
    println!("W7A|file={}|ratio_all={:.3}|p_mean_all={:.3}|ratio_in={:.3}|ratio_out={:.3}|nf={:.1}",
        path, ratio_all, p_mean_all, ratio_in, ratio_out, noise_floor);
}
