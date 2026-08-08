use hound::WavReader;
use std::path::{Path, PathBuf};
use std::fs;
use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{FixedPriors, VadClassifier};
use sp314_dsp::analysis::phi1_sensor::{Phi2StreamingFrontend, Phi2Pcen, Phi2Sensor};

fn median(mut a: Vec<f32>) -> f32 {
    if a.is_empty() { return 0.0; }
    a.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mid = a.len() / 2;
    if a.len() % 2 == 0 {
        (a[mid - 1] + a[mid]) / 2.0
    } else {
        a[mid]
    }
}

// 4 λεπτά — τρέχει χειροκίνητα:
// cargo test --release -p sp314-dsp
//   --test phi1_vs_dsp_jury -- --ignored --nocapture
#[ignore]
#[test]
fn test_phi1_vs_dsp_jury() {
    let beds_dir = Path::new("/tmp/w7a/beds");
    if !beds_dir.exists() {
        println!("SKIPPED: {} not found", beds_dir.display());
        return;
    }

    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(beds_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wav") {
                files.push(path);
            }
        }
    }
    files.sort();
    files.push(PathBuf::from("/tmp/w7a/fixtures/duck_splice_synth_snr-15.flac"));

    let results = std::sync::Mutex::new(Vec::new());

    println!("{:<45} | {:<10} | {:<10}", "filename", "dsp_ratio", "phi1_ratio");
    println!("{:-<45}-|-{:-<10}-|-{:-<10}", "", "", "");

    use rayon::prelude::*;
    files.par_iter().for_each(|path| {
        let filename = path.file_name().unwrap().to_string_lossy();
        let display_name = if filename.len() > 45 {
            filename[0..45].to_string()
        } else {
            filename.to_string()
        };

        if path.extension().and_then(|e| e.to_str()) == Some("flac") {
            results.lock().unwrap().push(format!("{:<45} | SKIPPED (flac unsupported by hound)", display_name));
            return;
        }

        let mut reader = match WavReader::open(&path) {
            Ok(r) => r,
            Err(_) => {
                results.lock().unwrap().push(format!("{:<45} | ERROR reading wav", display_name));
                return;
            }
        };
        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
            hound::SampleFormat::Int => reader.samples::<i16>().map(|s| s.unwrap_or(0) as f32).collect(),
        };

        let mut mono = Vec::with_capacity(samples.len() / spec.channels as usize);
        let mut left = Vec::with_capacity(samples.len() / spec.channels as usize);
        let mut right = Vec::with_capacity(samples.len() / spec.channels as usize);

        if spec.channels == 1 {
            mono = samples.clone();
            left = samples.clone();
            right = samples.clone();
        } else if spec.channels == 2 {
            for chunk in samples.chunks_exact(2) {
                left.push(chunk[0]);
                right.push(chunk[1]);
                mono.push((chunk[0] + chunk[1]) * 0.5);
            }
        }

        // Old pipeline
        let mut extractor = VadFeatureExtractor::new();
        let mut classifier = VadClassifier::new(FixedPriors);
        let features = extractor.process_chunk(&mono, &left, &right);
        
        let mut dsp_above = 0;
        let dsp_total = features.len();
        let noise_floor = -144.0;
        
        for f in features {
            let decision = classifier.process(&f, noise_floor);
            if decision.posterior > 0.5 {
                dsp_above += 1;
            }
        }

        // New pipeline
        let mut fe = Phi2StreamingFrontend::new();
        let mut pcen = Phi2Pcen::new();
        let mut sensor = Phi2Sensor::new();

        let mut phi1_above = 0;
        let mut phi1_total = 0;

        let frames1 = fe.push(&mono);
        let frames2 = fe.finish();
        
        for mel in frames1.iter().chain(frames2.iter()) {
            let pcen_frame = pcen.process(mel);
            if let Some(p) = sensor.push_frame(&pcen_frame) {
                phi1_total += 1;
                if p > 0.5 {
                    phi1_above += 1;
                }
            }
        }

        let dsp_ratio = if dsp_total > 0 { dsp_above as f32 / dsp_total as f32 } else { 0.0 };
        let phi1_ratio = if phi1_total > 0 { phi1_above as f32 / phi1_total as f32 } else { 0.0 };

        results.lock().unwrap().push(format!("{:<45} | {:<10.4} | {:<10.4}", display_name, dsp_ratio, phi1_ratio));
    });

    let mut output = results.into_inner().unwrap();
    output.sort();
    
    let mut dsp_ratios = Vec::new();
    let mut phi1_ratios = Vec::new();

    for line in output {
        println!("{}", line);
        if let Some(parts) = line.split(" | ").collect::<Vec<_>>().get(1..3) {
            if let (Ok(d), Ok(p)) = (parts[0].trim().parse::<f32>(), parts[1].trim().parse::<f32>()) {
                dsp_ratios.push(d);
                phi1_ratios.push(p);
            }
        }
    }

    let median_dsp = median(dsp_ratios);
    let median_phi1 = median(phi1_ratios);
    
    println!();
    println!("MEDIAN dsp_ratio:  {:.4}", median_dsp);
    println!("MEDIAN phi1_ratio: {:.4}", median_phi1);

    assert!(median_phi1 < 0.05, "phi1 median FP = {}", median_phi1);
    assert!(median_dsp > median_phi1 * 5.0, "phi1 should be dramatically cleaner");
}
