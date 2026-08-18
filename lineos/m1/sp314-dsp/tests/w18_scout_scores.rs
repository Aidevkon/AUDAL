use sp314_dsp::analysis::scout_scanner::scan_file;
use std::path::Path;

fn load_flight_clip_stereo(path: &str) -> (Vec<f32>, Vec<f32>, u32) {
    let mut reader = hound::WavReader::open(path).expect("failed to open clip");
    let spec = reader.spec();
    
    let mut left = Vec::new();
    let mut right = Vec::new();

    if spec.sample_format == hound::SampleFormat::Float {
        let samples: Vec<f32> = reader.samples::<f32>().map(|s: Result<f32, _>| s.unwrap()).collect();
        if spec.channels == 2 {
            for chunk in samples.chunks_exact(2) {
                left.push(chunk[0]);
                right.push(chunk[1]);
            }
        } else {
            left = samples.clone();
            right = samples;
        }
    } else {
        let samples: Vec<i16> = reader.samples::<i16>().map(|s: Result<i16, _>| s.unwrap()).collect();
        if spec.channels == 2 {
            for chunk in samples.chunks_exact(2) {
                left.push(chunk[0] as f32 / 32768.0);
                right.push(chunk[1] as f32 / 32768.0);
            }
        } else {
            let samples_f: Vec<f32> = samples.into_iter().map(|s| s as f32 / 32768.0).collect();
            left = samples_f.clone();
            right = samples_f;
        }
    }

    (left, right, spec.sample_rate)
}

#[test]
fn test_print_scout_scores() {
    let fixtures = vec![
        "/tmp/w9/podcast_realistic.wav",
        "/tmp/blue/comp0.wav",
    ];

    for path in fixtures {
        if !Path::new(path).exists() {
            println!("SKIPPING {}: missing", path);
            continue;
        }
        
        let (left, right, sr) = load_flight_clip_stereo(path);
        let decisions = scan_file(&left, &right, sr);
        
        println!("\n=== {} ===", path);
        if decisions.is_empty() {
            println!("No decisions returned!");
            continue;
        }
        
        // Scout is usually integrated by smoothing over time.
        // Let's print the average and min/max.
        let mut sum_lean = 0.0;
        let mut sum_conf = 0.0;
        let mut min_lean = 1.0_f32;
        let mut max_lean = 0.0_f32;
        
        for (t, d) in &decisions {
            sum_lean += d.leaning_score;
            sum_conf += d.confidence;
            if d.leaning_score < min_lean { min_lean = d.leaning_score; }
            if d.leaning_score > max_lean { max_lean = d.leaning_score; }
            // print a few samples
            if *t < 15.0 {
                println!("t={:.1}s: lean={:.4}, conf={:.4}", t, d.leaning_score, d.confidence);
            }
        }
        
        let avg_lean = sum_lean / decisions.len() as f32;
        let avg_conf = sum_conf / decisions.len() as f32;
        
        println!("OVERALL AVG: lean={:.4}, conf={:.4} (min={:.4}, max={:.4})", 
                 avg_lean, avg_conf, min_lean, max_lean);
    }
}
