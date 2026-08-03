use hound;
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{VadClassifier, FixedPriors};

fn load_wav(path: &str) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap();
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
    let base = "research/musdb-lab/excerpts";
    let tracks = ["Al_James_-_Schoolboy_Facination", "Forkupines_-_Semantics", "Punkdisco_-_Oral_Hygiene"];
    let stems = ["vocals", "drums", "bass", "other"];
    
    let mut all_results = Vec::new();
    
    for track in tracks {
        for stem in stems {
            let label = if stem == "vocals" { "V" } else if stem == "drums" { "D" } else { "M" };
            let path = format!("{}/{}/{}.wav", base, track, stem);
            let (mono, left, right) = load_wav(&path);
            
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
            
            let mut speech_frames = 0;
            let mut sum_p = 0.0;
            let mut max_p = 0.0_f32;
            
            for f in &features {
                let d = classifier.process(f, noise_floor);
                if d.is_speech {
                    speech_frames += 1;
                }
                sum_p += d.posterior;
                if d.posterior > max_p {
                    max_p = d.posterior;
                }
            }
            
            let speech_frame_ratio = if features.is_empty() { 0.0 } else { speech_frames as f32 / features.len() as f32 };
            let p_mean = if features.is_empty() { 0.0 } else { sum_p / features.len() as f32 };
            let p_max = max_p;
            
            let short_file = format!("{}_{}.wav", &track[..10], stem);
            
            all_results.push((short_file.clone(), label, speech_frame_ratio, p_mean, p_max, noise_floor));
            
            println!("MVAD|file={:<25}|label={}|speech_frame_ratio={:.3}|p_mean={:.3}|p_max={:.3}|nf={:.1}", 
                short_file, label, speech_frame_ratio, p_mean, p_max, noise_floor);
        }
    }
    
    println!("\n--- MVAD AGGREGATES ---");
    let classes = ["V", "D", "M"];
    
    for class in classes {
        let mut ratios = Vec::new();
        let mut pmeans = Vec::new();
        let mut pmaxs = Vec::new();
        for (_, label, ratio, pmean, pmax, _) in &all_results {
            if *label == class {
                ratios.push(*ratio);
                pmeans.push(*pmean);
                pmaxs.push(*pmax);
            }
        }
        
        let min_ratio = ratios.iter().copied().fold(f32::INFINITY, f32::min);
        let max_ratio = ratios.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean_ratio = ratios.iter().sum::<f32>() / ratios.len() as f32;
        
        let min_pmean = pmeans.iter().copied().fold(f32::INFINITY, f32::min);
        let max_pmean = pmeans.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean_pmean = pmeans.iter().sum::<f32>() / pmeans.len() as f32;

        let min_pmax = pmaxs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_pmax = pmaxs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean_pmax = pmaxs.iter().sum::<f32>() / pmaxs.len() as f32;
        
        println!("CLASS {} | Ratio min={:.3} mean={:.3} max={:.3} | P_mean min={:.3} mean={:.3} max={:.3} | P_max min={:.3} mean={:.3} max={:.3}",
            class, min_ratio, mean_ratio, max_ratio, min_pmean, mean_pmean, max_pmean, min_pmax, mean_pmax, max_pmax);
    }
}
