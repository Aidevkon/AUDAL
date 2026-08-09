use std::path::Path;
use hound::WavReader;
use sp314_dsp::analysis::phi1_sensor::{Phi2Pcen, Phi2StreamingFrontend};

#[test]
#[ignore]
fn w14_pcen_warmup() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    if !input_path.exists() {
        println!("SKIPPED: /tmp/w9/podcast_realistic.wav missing");
        return;
    }

    let mut reader = match WavReader::open(input_path) {
        Ok(r) => r,
        Err(_) => {
            println!("SKIPPED: /tmp/w9/podcast_realistic.wav missing");
            return;
        }
    };

    let spec = reader.spec();
    let channels = spec.channels as usize;
    let mut mono: Vec<f32> = Vec::new();

    match spec.sample_format {
        hound::SampleFormat::Float => {
            let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
            if channels == 1 {
                mono = samples;
            } else {
                for chunk in samples.chunks(channels) {
                    let sum: f32 = chunk.iter().sum();
                    mono.push(sum / channels as f32);
                }
            }
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_val = (1i64 << (bits - 1)) as f32;
            let samples: Vec<i32> = reader.samples::<i32>().map(|s| s.unwrap()).collect();
            if channels == 1 {
                mono = samples.into_iter().map(|s| s as f32 / max_val).collect();
            } else {
                for chunk in samples.chunks(channels) {
                    let sum: f32 = chunk.iter().map(|&s| s as f32 / max_val).sum();
                    mono.push(sum / channels as f32);
                }
            }
        }
    }

    // 1. ΑΝΑΦΟΡΑ
    let mut fe = Phi2StreamingFrontend::new();
    let mut mel_frames = fe.push(&mono);
    mel_frames.extend(fe.finish());

    let mut ref_pcen = Phi2Pcen::new();
    let ref_pcen_frames: Vec<[f32; 64]> = mel_frames
        .iter()
        .map(|frame| ref_pcen.process(frame))
        .collect();

    let total_mel = mel_frames.len();
    let chunk_frames_count = 137;
    let warmups = [0, 25, 50, 100, 200, 400];

    println!("warmup | max_abs_diff | mean_abs_diff | frames>1e-3");

    for &w in &warmups {
        let mut block_pcen_frames = Vec::with_capacity(total_mel);
        let mut start_idx = 0;

        while start_idx < total_mel {
            let end_idx = (start_idx + chunk_frames_count).min(total_mel);
            let block = &mel_frames[start_idx..end_idx];

            let warmup_start = start_idx.saturating_sub(w);
            let warmup_slice = &mel_frames[warmup_start..start_idx];

            let mut pcen = Phi2Pcen::new();
            for frame in warmup_slice {
                let _ = pcen.process(frame);
            }

            for frame in block {
                block_pcen_frames.push(pcen.process(frame));
            }

            start_idx = end_idx;
        }

        let mut max_abs_diff = 0.0f32;
        let mut sum_abs_diff = 0.0f64;
        let mut count_gt_1e3 = 0usize;
        let total_elements = total_mel * 64;

        for (ref_frame, block_frame) in ref_pcen_frames.iter().zip(block_pcen_frames.iter()) {
            let mut frame_has_diff_gt_1e3 = false;
            for c in 0..64 {
                let diff = (ref_frame[c] - block_frame[c]).abs();
                if diff > max_abs_diff {
                    max_abs_diff = diff;
                }
                sum_abs_diff += diff as f64;
                if diff > 1e-3 {
                    frame_has_diff_gt_1e3 = true;
                }
            }
            if frame_has_diff_gt_1e3 {
                count_gt_1e3 += 1;
            }
        }

        let mean_abs_diff = (sum_abs_diff / (total_elements as f64)) as f32;

        println!(
            "{:6} | {:14.6e} | {:13.6e} | {:11}",
            w, max_abs_diff, mean_abs_diff, count_gt_1e3
        );
    }
}
