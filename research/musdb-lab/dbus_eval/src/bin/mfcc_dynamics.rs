use hound;
use lineos_corpus::mfcc::MfccAnalyzer;
use std::fs;

fn load_mono_wav(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    let channels = reader.spec().channels;
    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
    
    let mut mono = Vec::new();
    if channels == 2 {
        for chunk in samples.chunks_exact(2) {
            mono.push((chunk[0] + chunk[1]) * 0.5);
        }
    } else {
        mono = samples;
    }
    mono
}

struct MfccFeatures {
    file: String,
    cls: String,
    std: [f32; 13],
    delta_mean: [f32; 13],
    dyn_low: f32,
    dyn_high: f32,
}

fn process_file(path: &str, cls: &str, short_name: &str) -> Option<MfccFeatures> {
    let mono = load_mono_wav(path);
    let mut analyzer = MfccAnalyzer::new();
    
    let mut windows = Vec::new();
    let hop = 1024;
    let mut start = 0;
    while start + 1024 <= mono.len() {
        let chunk = &mono[start..start + 1024];
        let mfcc = analyzer.compute(chunk);
        windows.push(mfcc);
        start += hop;
    }
    
    if windows.is_empty() { return None; }
    
    // std
    let mut mean = [0.0f32; 13];
    for w in &windows {
        for i in 0..13 { mean[i] += w[i]; }
    }
    for i in 0..13 { mean[i] /= windows.len() as f32; }
    
    let mut std = [0.0f32; 13];
    for w in &windows {
        for i in 0..13 {
            let diff = w[i] - mean[i];
            std[i] += diff * diff;
        }
    }
    for i in 0..13 { std[i] = (std[i] / windows.len() as f32).sqrt(); }
    
    // delta mean
    let mut delta_mean = [0.0f32; 13];
    if windows.len() > 1 {
        for t in 1..windows.len() {
            for i in 0..13 {
                delta_mean[i] += (windows[t][i] - windows[t-1][i]).abs();
            }
        }
        for i in 0..13 {
            delta_mean[i] /= (windows.len() - 1) as f32;
        }
    }
    
    // scalars
    let dyn_low = (delta_mean[1] + delta_mean[2] + delta_mean[3] + delta_mean[4]) / 4.0;
    let dyn_high = (delta_mean[5] + delta_mean[6] + delta_mean[7] + delta_mean[8] + delta_mean[9] + delta_mean[10] + delta_mean[11] + delta_mean[12]) / 8.0;
    
    println!("DYN|class={}|dyn_low={:.4}|dyn_high={:.4} ({})", cls, dyn_low, dyn_high, short_name);
    
    Some(MfccFeatures {
        file: short_name.to_string(),
        cls: cls.to_string(),
        std,
        delta_mean,
        dyn_low,
        dyn_high,
    })
}

fn main() {
    let mut results = Vec::new();
    
    // SPEECH
    let speech_dir = "research/musdb-lab/speech_resampled";
    if let Ok(entries) = fs::read_dir(speech_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("wav") {
                if let Some(res) = process_file(entry.path().to_str().unwrap(), "SPEECH", entry.file_name().to_str().unwrap()) {
                    results.push(res);
                }
            }
        }
    }
    
    // SUNG & INSTR
    let excerpts_dir = "research/musdb-lab/excerpts";
    if let Ok(entries) = fs::read_dir(excerpts_dir) {
        for entry in entries.flatten() {
            if entry.file_type().unwrap().is_dir() {
                let track = entry.file_name().into_string().unwrap();
                
                let v_path = entry.path().join("vocals.wav");
                if v_path.exists() {
                    let name = format!("{}_vocals", track.chars().take(10).collect::<String>());
                    if let Some(res) = process_file(v_path.to_str().unwrap(), "SUNG", &name) {
                        results.push(res);
                    }
                }
                
                let b_path = entry.path().join("bass.wav");
                if b_path.exists() {
                    let name = format!("{}_bass", track.chars().take(10).collect::<String>());
                    if let Some(res) = process_file(b_path.to_str().unwrap(), "INSTR", &name) {
                        results.push(res);
                    }
                }
                
                let o_path = entry.path().join("other.wav");
                if o_path.exists() {
                    let name = format!("{}_other", track.chars().take(10).collect::<String>());
                    if let Some(res) = process_file(o_path.to_str().unwrap(), "INSTR", &name) {
                        results.push(res);
                    }
                }
            }
        }
    }
    
    // Aggregates
    let classes = ["SPEECH", "SUNG", "INSTR"];
    for cls in &classes {
        let mut lows = Vec::new();
        let mut highs = Vec::new();
        for r in &results {
            if &r.cls == cls {
                lows.push(r.dyn_low);
                highs.push(r.dyn_high);
            }
        }
        if lows.is_empty() { continue; }
        let low_min = lows.iter().copied().fold(f32::INFINITY, f32::min);
        let low_max = lows.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let low_mean: f32 = lows.iter().sum::<f32>() / lows.len() as f32;
        let high_min = highs.iter().copied().fold(f32::INFINITY, f32::min);
        let high_max = highs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let high_mean: f32 = highs.iter().sum::<f32>() / highs.len() as f32;
        println!("AGG|class={}|dyn_low_min={:.4}|dyn_low_mean={:.4}|dyn_low_max={:.4}|dyn_high_min={:.4}|dyn_high_mean={:.4}|dyn_high_max={:.4}", 
                 cls, low_min, low_mean, low_max, high_min, high_mean, high_max);
    }
    
    // Gaps and overlap
    let mut speech_min_dl = f32::INFINITY; let mut speech_max_dl = f32::NEG_INFINITY;
    let mut sung_min_dl = f32::INFINITY; let mut sung_max_dl = f32::NEG_INFINITY;
    let mut instr_min_dl = f32::INFINITY; let mut instr_max_dl = f32::NEG_INFINITY;
    
    let mut speech_min_dh = f32::INFINITY; let mut speech_max_dh = f32::NEG_INFINITY;
    let mut sung_min_dh = f32::INFINITY; let mut sung_max_dh = f32::NEG_INFINITY;
    let mut instr_min_dh = f32::INFINITY; let mut instr_max_dh = f32::NEG_INFINITY;
    
    for r in &results {
        if r.cls == "SPEECH" {
            speech_min_dl = speech_min_dl.min(r.dyn_low); speech_max_dl = speech_max_dl.max(r.dyn_low);
            speech_min_dh = speech_min_dh.min(r.dyn_high); speech_max_dh = speech_max_dh.max(r.dyn_high);
        } else if r.cls == "SUNG" {
            sung_min_dl = sung_min_dl.min(r.dyn_low); sung_max_dl = sung_max_dl.max(r.dyn_low);
            sung_min_dh = sung_min_dh.min(r.dyn_high); sung_max_dh = sung_max_dh.max(r.dyn_high);
        } else if r.cls == "INSTR" {
            instr_min_dl = instr_min_dl.min(r.dyn_low); instr_max_dl = instr_max_dl.max(r.dyn_low);
            instr_min_dh = instr_min_dh.min(r.dyn_high); instr_max_dh = instr_max_dh.max(r.dyn_high);
        }
    }
    
    let voice_min_dl = speech_min_dl.min(sung_min_dl);
    let voice_min_dh = speech_min_dh.min(sung_min_dh);
    
    let gap_low = voice_min_dl - instr_max_dl;
    let gap_high = voice_min_dh - instr_max_dh;
    
    println!("GAP|dyn_low={:.4}|dyn_high={:.4}", gap_low, gap_high);
    
    println!("QUESTIONS:");
    println!("(a) Do SPEECH and SUNG overlap?");
    println!("    dyn_low: SPEECH [{:.4}, {:.4}], SUNG [{:.4}, {:.4}]", speech_min_dl, speech_max_dl, sung_min_dl, sung_max_dl);
    println!("    dyn_high: SPEECH [{:.4}, {:.4}], SUNG [{:.4}, {:.4}]", speech_min_dh, speech_max_dh, sung_min_dh, sung_max_dh);
    
    println!("(b) Does {{SPEECH + SUNG}} separate from INSTR?");
    println!("    dyn_low VOICE min: {:.4} vs INSTR max: {:.4} -> GAP: {:.4}", voice_min_dl, instr_max_dl, gap_low);
    println!("    dyn_high VOICE min: {:.4} vs INSTR max: {:.4} -> GAP: {:.4}", voice_min_dh, instr_max_dh, gap_high);
    
    // Per-coefficient analysis
    println!("\nPer-coefficient delta_mean gaps (VOICE min - INSTR max):");
    for i in 1..13 {
        let mut vmin = f32::INFINITY;
        let mut imax = f32::NEG_INFINITY;
        for r in &results {
            if r.cls == "SPEECH" || r.cls == "SUNG" {
                vmin = vmin.min(r.delta_mean[i]);
            } else if r.cls == "INSTR" {
                imax = imax.max(r.delta_mean[i]);
            }
        }
        println!("c{}: {:.4}", i, vmin - imax);
    }
}
