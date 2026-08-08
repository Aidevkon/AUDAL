use sp314_dsp::analysis::phi1_sensor::{Phi1MelFrontend, Phi2Pcen, Phi2Sensor};
use hound::WavReader;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::Command;

#[test]
fn test_phi2_sensor_equivalence() {
    let wav_path = "/tmp/phi1/eq_test_48k.wav";
    let dump_path = "/tmp/phi2/eq_p_48k.csv";

    if !std::path::Path::new(wav_path).exists() {
        println!("Skipping equivalence test, missing {}", wav_path);
        return;
    }

    std::fs::create_dir_all("/tmp/phi2").unwrap();

    let output = Command::new("cargo")
        .args(&[
            "run", "--release", "--bin", "mvad_phi1", "--manifest-path",
            "../../../research/musdb-lab/dbus_eval/Cargo.toml", "--",
            wav_path, "--frontend=pcen",
            "--weights=../../../research/musdb-lab/assets/phi2_pcen.bin",
            &format!("--dump={}", dump_path)
        ])
        .output()
        .expect("Failed to run baseline");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("PHI1|") {
            println!("{}", line);
        }
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
    let frames = frontend.compute_all_power(&mono);
    let mut pcen = Phi2Pcen::new();
    let mut sensor = Phi2Sensor::new();

    let mut module_p = Vec::new();
    for frame in &frames {
        let pcen_frame = pcen.process(frame);
        if let Some(p) = sensor.push_frame(&pcen_frame) {
            module_p.push(p);
        }
    }

    let file = File::open(dump_path).unwrap();
    let buf_reader = BufReader::new(file);
    let mut baseline_p = Vec::new();
    for (i, line) in buf_reader.lines().enumerate() {
        let l = line.unwrap();
        let parts: Vec<&str> = l.split(',').collect();
        if parts.len() >= 2 {
            if let Ok(p) = parts[parts.len() - 1].parse::<f32>() {
                baseline_p.push(p);
            }
        }
    }

    let mut max_diff = 0.0f32;
    let min_len = module_p.len().min(baseline_p.len());
    for i in 0..min_len {
        let diff = (module_p[i] - baseline_p[i]).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }

    if module_p.len() != baseline_p.len() || max_diff >= 1e-4 {
        println!("Max diff: {}", max_diff);
        println!("Counts: {} (module) vs {} (baseline)", module_p.len(), baseline_p.len());
        println!("Frame 0: module {} baseline {}", module_p.get(0).unwrap_or(&-1.0), baseline_p.get(0).unwrap_or(&-1.0));
        println!("Frame 50: module {} baseline {}", module_p.get(50).unwrap_or(&-1.0), baseline_p.get(50).unwrap_or(&-1.0));
        println!("Frame 150: module {} baseline {}", module_p.get(150).unwrap_or(&-1.0), baseline_p.get(150).unwrap_or(&-1.0));
        panic!("Equivalence failed: counts match={} max|Δp|={}", module_p.len() == baseline_p.len(), max_diff);
    }
}
