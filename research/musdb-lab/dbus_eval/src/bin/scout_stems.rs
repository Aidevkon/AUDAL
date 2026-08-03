use hound;
use sp314_dsp::analysis::scout_scanner::scan_file;
use sp314_dsp::analysis::scout::SegmentScout;

fn load_wav(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    let channels = reader.spec().channels;
    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
    if channels == 2 {
        let mut mono = Vec::with_capacity(samples.len() / 2);
        for i in 0..(samples.len() / 2) {
            mono.push((samples[i * 2] + samples[i * 2 + 1]) * 0.5);
        }
        mono
    } else {
        samples
    }
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
            let audio = load_wav(&path);
            
            // 1) Get the decisions via the scanner
            let decisions = scan_file(&audio, &audio, 48000);
            
            let mut total_lean = 0.0;
            let mut total_conf = 0.0;
            for (_, d) in &decisions {
                total_lean += d.leaning_score;
                total_conf += d.confidence;
            }
            
            let mean_lean = if decisions.is_empty() { 0.0 } else { total_lean / decisions.len() as f32 };
            let mean_conf = if decisions.is_empty() { 0.0 } else { total_conf / decisions.len() as f32 };
            
            // 2) Run the raw scout loop to get cv_ioi and cepstral_flux
            let win_samples = (5.0 * 48000.0) as usize;
            let hop_samples = (1.0 * 48000.0) as usize;
            let mut scout = SegmentScout::new();
            let mut start = 0;
            
            let mut total_cv = 0.0;
            let mut valid_cvs = 0;
            let mut total_flux = 0.0;
            let mut valid_fluxes = 0;

            while start + win_samples <= audio.len() {
                let mono_slice = &audio[start..start + win_samples];
                
                let mut mfcc_analyzer = lineos_corpus::mfcc::MfccAnalyzer::new();
                let mut mfccs = Vec::new();
                let mut f = 0;
                while f + 1024 <= mono_slice.len() {
                    mfccs.push(mfcc_analyzer.compute(&mono_slice[f..f + 1024]));
                    f += 512;
                }
                let cepstral_flux = lineos_corpus::scout::compute_cepstral_flux(&mfccs);
                
                let meas = scout.measure(mono_slice, cepstral_flux, 48000);
                
                total_flux += meas.cepstral_flux;
                valid_fluxes += 1;
                
                if !meas.cv_ioi.is_nan() {
                    total_cv += meas.cv_ioi;
                    valid_cvs += 1;
                }
                
                start += hop_samples;
            }
            
            let mean_cv = if valid_cvs == 0 { f32::NAN } else { total_cv / valid_cvs as f32 };
            let mean_flux = if valid_fluxes == 0 { 0.0 } else { total_flux / valid_fluxes as f32 };
            
            let short_file = format!("{}_{}.wav", &track[..10], stem);
            
            all_results.push((short_file.clone(), label, mean_lean, mean_conf, mean_cv, mean_flux));
            
            println!("SCOUT|file={:<25}|label={}|leaning={:.3}|confidence={:.3}|cv_ioi={:.3}|cepstral_flux={:.3}", 
                short_file, label, mean_lean, mean_conf, mean_cv, mean_flux);
        }
    }
    
    println!("\n--- SCOUT AGGREGATES ---");
    let classes = ["V", "D", "M"];
    
    for class in classes {
        let mut leans = Vec::new();
        let mut confs = Vec::new();
        for (_, label, lean, conf, _, _) in &all_results {
            if *label == class {
                leans.push(*lean);
                confs.push(*conf);
            }
        }
        
        let min_lean = leans.iter().copied().fold(f32::INFINITY, f32::min);
        let max_lean = leans.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean_lean = leans.iter().sum::<f32>() / leans.len() as f32;
        
        let min_conf = confs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_conf = confs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean_conf = confs.iter().sum::<f32>() / confs.len() as f32;
        
        println!("CLASS {} | Leaning min={:.3} mean={:.3} max={:.3} | Conf min={:.3} mean={:.3} max={:.3}",
            class, min_lean, mean_lean, max_lean, min_conf, mean_conf, max_conf);
    }
}
