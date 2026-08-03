use hound;
use lineos_corpus::mfcc::MfccAnalyzer;

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

fn euclidean_distance(a: &[f32; 13], b: &[f32; 13]) -> f32 {
    let mut sum = 0.0;
    for i in 0..13 {
        let diff = a[i] - b[i];
        sum += diff * diff;
    }
    sum.sqrt()
}

fn main() {
    let base = "research/musdb-lab/excerpts";
    let tracks = ["Al_James_-_Schoolboy_Facination", "Forkupines_-_Semantics", "Punkdisco_-_Oral_Hygiene"];
    let stems = ["vocals", "drums", "bass", "other"];
    
    let mut raw_mfccs = Vec::new();
    
    for track in tracks {
        for stem in stems {
            let label = if stem == "vocals" { "V" } else if stem == "drums" { "D" } else { "M" };
            let path = format!("{}/{}/{}.wav", base, track, stem);
            let mono = load_mono_wav(&path);
            
            let mut analyzer = MfccAnalyzer::new();
            let mfcc = analyzer.compute_windowed(&mono);
            
            let short_file = format!("{}_{}.wav", &track[..10], stem);
            
            print!("MFCC|file={:<25}|label={}", short_file, label);
            for (i, c) in mfcc.iter().enumerate() {
                print!("|c{}={:.3}", i, c);
            }
            println!();
            
            raw_mfccs.push((short_file.clone(), label, mfcc));
        }
    }
    
    // Calculate global mean and std for these 12 files
    let mut means = [0.0f32; 13];
    for (_, _, mfcc) in &raw_mfccs {
        for i in 0..13 {
            means[i] += mfcc[i];
        }
    }
    for i in 0..13 {
        means[i] /= 12.0;
    }
    
    let mut variances = [0.0f32; 13];
    for (_, _, mfcc) in &raw_mfccs {
        for i in 0..13 {
            let diff = mfcc[i] - means[i];
            variances[i] += diff * diff;
        }
    }
    let mut stds = [0.0f32; 13];
    for i in 0..13 {
        stds[i] = (variances[i] / 12.0).sqrt();
        if stds[i] < 1e-6 { stds[i] = 1.0; }
    }
    
    // Z-score the MFCCs
    let mut z_mfccs = Vec::new();
    for (file, label, mfcc) in &raw_mfccs {
        let mut z = [0.0f32; 13];
        for i in 0..13 {
            z[i] = (mfcc[i] - means[i]) / stds[i];
        }
        z_mfccs.push((file.clone(), *label, z));
    }
    
    // Calculate Centroids (Vocal=V, Instr=M)
    let mut voice_centroid = [0.0f32; 13];
    let mut instr_centroid = [0.0f32; 13];
    let mut voice_count = 0;
    let mut instr_count = 0;
    
    for (_, label, z) in &z_mfccs {
        if *label == "V" {
            for i in 0..13 { voice_centroid[i] += z[i]; }
            voice_count += 1;
        } else if *label == "M" {
            for i in 0..13 { instr_centroid[i] += z[i]; }
            instr_count += 1;
        }
    }
    
    for i in 0..13 {
        if voice_count > 0 { voice_centroid[i] /= voice_count as f32; }
        if instr_count > 0 { instr_centroid[i] /= instr_count as f32; }
    }
    
    println!("\n--- DISTANCES TO PROVISIONAL CENTROIDS ---");
    println!("Note: Z-scores and centroids are LOCAL to these 12 files (optimistic by construction).");
    
    let mut v_correct = 0;
    let mut m_correct = 0;
    let mut min_margin_v = f32::INFINITY;
    let mut min_margin_m = f32::INFINITY;
    
    for (file, label, z) in &z_mfccs {
        let d_voice = euclidean_distance(z, &voice_centroid);
        let d_instr = euclidean_distance(z, &instr_centroid);
        let nearest = if d_voice < d_instr { "V" } else { "M" };
        let margin = (d_voice - d_instr).abs();
        
        println!("DIST|file={:<25}|label={}|d_voice={:.3}|d_instr={:.3}|nearest={}|margin={:.3}",
            file, label, d_voice, d_instr, nearest, margin);
            
        if *label == "V" {
            if nearest == "V" { v_correct += 1; }
            if margin < min_margin_v { min_margin_v = margin; }
        } else if *label == "M" {
            if nearest == "M" { m_correct += 1; }
            if margin < min_margin_m { min_margin_m = margin; }
        }
    }
    
    println!("\nRESULTS:");
    println!("Vocals landed nearest Voice Centroid: {} / 3 (Min Margin: {:.3})", v_correct, min_margin_v);
    println!("Instrs landed nearest Instr Centroid: {} / 6 (Min Margin: {:.3})", m_correct, min_margin_m);
}
