use hound;
use sp314_dsp::stft::StftEngine;
use sp314_dsp::analysis::role::{low_ratio, spectral_flux, classify, RoleFeatures, Role};
use std::collections::HashMap;

fn load_wav(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    let channels = reader.spec().channels;
    // Just map to mono by averaging if needed, or assume they are already mono if we mapped them?
    // Wait, MUSDB excerpts are stereo! The python code `role_features.py` does `if audio.ndim > 1: audio = np.mean(audio, axis=1)`.
    // I need to average them to mono.
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
    
    // Hardcoded lab values (low, flux)
    let mut lab_values = HashMap::new();
    lab_values.insert("Al_James_-_Schoolboy_Facination_vocals", (0.03, 0.08));
    lab_values.insert("Al_James_-_Schoolboy_Facination_drums", (0.41, 0.08));
    lab_values.insert("Al_James_-_Schoolboy_Facination_bass", (0.56, 0.01));
    lab_values.insert("Al_James_-_Schoolboy_Facination_other", (0.10, 0.04));
    
    lab_values.insert("Forkupines_-_Semantics_vocals", (0.05, 0.06));
    lab_values.insert("Forkupines_-_Semantics_drums", (0.76, 0.11));
    lab_values.insert("Forkupines_-_Semantics_bass", (0.89, 0.02));
    lab_values.insert("Forkupines_-_Semantics_other", (0.15, 0.06));
    
    lab_values.insert("Punkdisco_-_Oral_Hygiene_vocals", (0.24, 0.02));
    lab_values.insert("Punkdisco_-_Oral_Hygiene_drums", (0.53, 0.07));
    lab_values.insert("Punkdisco_-_Oral_Hygiene_bass", (0.81, 0.03));
    lab_values.insert("Punkdisco_-_Oral_Hygiene_other", (0.17, 0.02));
    
    for track in tracks {
        for stem in stems {
            let label = if stem == "vocals" { "V" } else if stem == "drums" { "D" } else { "M" };
            let path = format!("{}/{}/{}.wav", base, track, stem);
            let audio = load_wav(&path);
            
            let mut stft = StftEngine::new();
            let (cplx, n_frames) = stft.forward(&audio);
            
            let mut low_list = Vec::new();
            let mut flux_list = Vec::new();
            
            let mut prev_mag = vec![0.0_f32; 1025];
            
            for f in 0..n_frames {
                let mut cur_mag = vec![0.0_f32; 1025];
                for b in 0..1025 {
                    let c = cplx[f][b];
                    cur_mag[b] = libm::sqrtf(c.re * c.re + c.im * c.im);
                }
                
                let low = low_ratio(&cur_mag, 48000.0);
                low_list.push(low);
                
                if f > 0 {
                    let flux = spectral_flux(&cur_mag, &prev_mag);
                    flux_list.push(flux);
                }
                
                prev_mag = cur_mag;
            }
            
            let mean_low = low_list.iter().sum::<f32>() / low_list.len() as f32;
            let mean_flux = flux_list.iter().sum::<f32>() / flux_list.len() as f32;
            
            let key = format!("{}_{}", track, stem);
            let (lab_low, lab_flux) = lab_values.get(key.as_str()).copied().unwrap();
            
            let features = RoleFeatures {
                speech_p: None,
                low_ratio: mean_low,
                flux: mean_flux,
            };
            
            let role = classify(&features);
            let role_str = match role {
                Role::Voice => "Voice",
                Role::Drums => "Drums",
                Role::Music => "Music",
            };
            
            let short_file = format!("{}_{}.wav", &track[..10], stem);
            
            let ratio_low = if lab_low > 0.0 { mean_low / lab_low } else { 0.0 };
            let ratio_flux = if lab_flux > 0.0 { mean_flux / lab_flux } else { 0.0 };
            
            println!("PARITY|file={:<25}|label={}|rust_low={:.3}|rust_flux={:.3}|lab_low={:.2}|lab_flux={:.2}|rust_role={:<5}|ratio_low={:.2}|ratio_flux={:.2}", 
                short_file, label, mean_low, mean_flux, lab_low, lab_flux, role_str, ratio_low, ratio_flux);
        }
    }
}
