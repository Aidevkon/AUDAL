use sp314_dsp::analysis::phi1_sensor::{Phi1MelFrontend, Phi1Sensor, compute_norm};
use hound::WavReader;
use std::fs::File;
use std::io::{BufRead, BufReader};

#[test]
fn phi1_sensor_equivalence() {
    let wav_path = "/tmp/phi1/eq_test_48k.wav";
    let csv_path = "/tmp/phi1/eq_p_48k.csv";

    if !std::path::Path::new(wav_path).exists() || !std::path::Path::new(csv_path).exists() {
        println!("Skipping equivalence test, missing {} or {}", wav_path, csv_path);
        return;
    }

    let mut reader = WavReader::open(wav_path).unwrap();
    let spec = reader.spec();
    let mut mono: Vec<f32> = Vec::new();

    if spec.sample_format == hound::SampleFormat::Float {
        let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
        for s in samples { mono.push(s); }
    } else {
        let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        for s in samples { mono.push(s as f32 / 32768.0); }
    }

    let mut frontend = Phi1MelFrontend::new();
    let frames = frontend.compute_all(&mono);
    let norm = compute_norm(&frames);
    let mut sensor = Phi1Sensor::new(norm);

    let mut p_rs = Vec::new();
    for frame in &frames {
        if let Some(p) = sensor.push_frame(frame) {
            p_rs.push(p);
        }
    }

    let file = File::open(csv_path).unwrap();
    let buf_reader = BufReader::new(file);
    let mut expected_p = Vec::new();
    for line in buf_reader.lines() {
        let l = line.unwrap();
        let parts: Vec<&str> = l.split(',').collect();
        if parts.len() == 2 {
            if let Ok(p) = parts[1].parse::<f32>() {
                expected_p.push(p);
            }
        }
    }

    let mut max_diff = 0.0f32;
    let len_min = p_rs.len().min(expected_p.len());
    for i in 0..len_min {
        let diff = (p_rs[i] - expected_p[i]).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }

    if p_rs.len() != expected_p.len() || max_diff >= 1e-4 {
        let f0 = if len_min > 0 { format!("{} vs {}", p_rs[0], expected_p[0]) } else { "N/A".to_string() };
        let f50 = if len_min > 50 { format!("{} vs {}", p_rs[50], expected_p[50]) } else { "N/A".to_string() };
        let f150 = if len_min > 150 { format!("{} vs {}", p_rs[150], expected_p[150]) } else { "N/A".to_string() };
        panic!("max|Δp| = {}, frames: {} (module) vs {} (baseline).\nFrames (module vs baseline):\n  0: {}\n 50: {}\n150: {}",
               max_diff, p_rs.len(), expected_p.len(), f0, f50, f150);
    }
}
