use sp314_dsp::restoration::{RestorationChain, RestorationConfig};
use std::f32::consts::PI;

fn ensure_output_dir() {
    std::fs::create_dir_all("tests/outputs").unwrap();
}

fn write_wav(name: &str, left: &[f32], right: &[f32]) {
    ensure_output_dir();
    let path = format!("tests/outputs/{}", name);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for i in 0..left.len() {
        writer.write_sample(left[i]).unwrap();
        writer.write_sample(right[i]).unwrap();
    }
    writer.finalize().unwrap();
}

fn measure_group_delay(config: RestorationConfig, gate_threshold_db: f32) -> usize {
    let mut chain = RestorationChain::new(48000.0, config, 0.0, gate_threshold_db);
    let mut left = vec![0.0; 4096];
    let mut right = vec![0.0; 4096];
    left[0] = 1.0;
    right[0] = 1.0;
    chain.process(&mut left, &mut right);
    
    let mut max_val = 0.0;
    let mut max_idx = 0;
    for i in 0..left.len() {
        let abs_val = left[i].abs();
        if abs_val > max_val {
            max_val = abs_val;
            max_idx = i;
        }
    }
    max_idx
}

fn render_triplet(
    name: &str,
    config: RestorationConfig,
    sig_l: &[f32],
    sig_r: &[f32],
    gate_threshold_db: f32,
    listen_for: &str,
) {
    write_wav(&format!("{}_dry.wav", name), sig_l, sig_r);
    
    let mut wet_l = sig_l.to_vec();
    let mut wet_r = sig_r.to_vec();
    
    let mut chain = RestorationChain::new(48000.0, config.clone(), 0.0, gate_threshold_db);
    chain.process(&mut wet_l, &mut wet_r);
    
    write_wav(&format!("{}_wet.wav", name), &wet_l, &wet_r);
    
    let mut delta_raw_l = vec![0.0; sig_l.len()];
    let mut delta_raw_r = vec![0.0; sig_r.len()];
    for i in 0..sig_l.len() {
        delta_raw_l[i] = sig_l[i] - wet_l[i];
        delta_raw_r[i] = sig_r[i] - wet_r[i];
    }
    write_wav(&format!("{}_delta_raw.wav", name), &delta_raw_l, &delta_raw_r);
    
    let delay = measure_group_delay(config, gate_threshold_db);
    
    let mut delta_aligned_l = vec![0.0; sig_l.len()];
    let mut delta_aligned_r = vec![0.0; sig_r.len()];
    for i in 0..sig_l.len() {
        let wet_idx = i + delay;
        let w_l = if wet_idx < wet_l.len() { wet_l[wet_idx] } else { 0.0 };
        let w_r = if wet_idx < wet_r.len() { wet_r[wet_idx] } else { 0.0 };
        delta_aligned_l[i] = sig_l[i] - w_l;
        delta_aligned_r[i] = sig_r[i] - w_r;
    }
    write_wav(&format!("{}_delta_aligned.wav", name), &delta_aligned_l, &delta_aligned_r);
    
    println!("\n=== {} ===", name);
    println!("Paths:");
    println!("  tests/outputs/{}_dry.wav", name);
    println!("  tests/outputs/{}_wet.wav", name);
    println!("  tests/outputs/{}_delta_raw.wav", name);
    println!("  tests/outputs/{}_delta_aligned.wav", name);
    println!("group delay: {} samples ({:.2}ms)", delay, delay as f32 / 48.0);
    println!("LISTEN FOR: {}", listen_for);
}

#[test]
#[ignore]
fn forensic_gate() {
    let mut sig_l = vec![0.0; 48000 * 3];
    let mut sig_r = vec![0.0; 48000 * 3];
    for i in 0..sig_l.len() {
        let t = i as f32 / 48000.0;
        let burst = if t % 1.3 < 0.8 { 1.0 } else { 0.0 };
        let noise = (t * 400.0 * 2.0 * PI).sin() * 0.1 
                  + (t * 800.0 * 2.0 * PI).sin() * 0.1 
                  + (t * 1500.0 * 2.0 * PI).sin() * 0.1;
        let base_noise = (t * 5000.0 * 2.0 * PI).sin() * 0.001;
        
        sig_l[i] = noise * burst + base_noise;
        sig_r[i] = noise * burst + base_noise;
    }
    let config = RestorationConfig {
        lowcut_enabled: false, hum_enabled: false, deess_enabled: false, gate_enabled: true
    };
    render_triplet("gate_45", config.clone(), &sig_l, &sig_r, -45.0, 
        "chatter at threshold, clicks on open/close, tail cutting; -45 aggression");
    render_triplet("gate_52", config, &sig_l, &sig_r, -52.0, 
        "chatter at threshold, clicks on open/close, tail cutting; -52 aggression");
}

#[test]
#[ignore]
fn forensic_deess() {
    let mut sig_l = vec![0.0; 48000 * 3];
    let mut sig_r = vec![0.0; 48000 * 3];
    for i in 0..sig_l.len() {
        let t = i as f32 / 48000.0;
        let bed = (t * 200.0 * 2.0 * PI).sin() * 0.125 + (t * 1000.0 * 2.0 * PI).sin() * 0.125;
        let sibilant_burst = if (t * 2.0).fract() < 0.4 { 1.0 } else { 0.0 };
        let sibilant = (t * 8000.0 * 2.0 * PI).sin() * sibilant_burst * 0.5;
        
        sig_l[i] = bed + sibilant;
        sig_r[i] = bed + sibilant * 0.2;
    }
    let config = RestorationConfig {
        lowcut_enabled: false, hum_enabled: false, deess_enabled: true, gate_enabled: false
    };
    render_triplet("deess", config, &sig_l, &sig_r, -45.0, 
        "bed pumping (delta should show ONLY the 8kHz sss, no bed), lisp, image wander");
}

#[test]
#[ignore]
fn forensic_lowcut() {
    let mut sig_l = vec![0.0; 48000 * 3];
    let mut sig_r = vec![0.0; 48000 * 3];
    for i in 0..sig_l.len() {
        let t = i as f32 / 48000.0;
        let voice = (t * 300.0 * 2.0 * PI).sin() * 0.3;
        let thump_burst = if t % 1.0 < 0.15 { 1.0 } else { 0.0 };
        let thump = (t * 40.0 * 2.0 * PI).sin() * thump_burst * 0.4;
        let rumble = (t * 20.0 * 2.0 * PI).sin() * 0.1;
        
        sig_l[i] = voice + thump + rumble;
        sig_r[i] = voice + thump + rumble;
    }
    let config = RestorationConfig {
        lowcut_enabled: true, hum_enabled: false, deess_enabled: false, gate_enabled: false
    };
    render_triplet("lowcut", config, &sig_l, &sig_r, -45.0, 
        "thumps/rumble cleanly gone (delta = only the lows), 300Hz body intact");
}

#[test]
#[ignore]
fn forensic_dehum() {
    let mut sig_l = vec![0.0; 48000 * 3];
    let mut sig_r = vec![0.0; 48000 * 3];
    for i in 0..sig_l.len() {
        let t = i as f32 / 48000.0;
        let hum = (t * 50.0 * 2.0 * PI).sin() * 0.15 
                + (t * 100.0 * 2.0 * PI).sin() * 0.15 
                + (t * 150.0 * 2.0 * PI).sin() * 0.15;
        let bass = (t * 55.0 * 2.0 * PI).sin() * 0.15 
                 + (t * 110.0 * 2.0 * PI).sin() * 0.15;
        let mid = (t * 500.0 * 2.0 * PI).sin() * 0.3;
        
        sig_l[i] = hum + bass + mid;
        sig_r[i] = hum + bass + mid;
    }
    let config = RestorationConfig {
        lowcut_enabled: false, hum_enabled: true, deess_enabled: false, gate_enabled: false
    };
    render_triplet("dehum", config, &sig_l, &sig_r, -45.0, 
        "delta should be CLEAN 50/100/150 sines only - no 55/110Hz leakage in delta");
}

#[test]
#[ignore]
fn forensic_combined() {
    let mut sig_l = vec![0.0; 48000 * 3];
    let mut sig_r = vec![0.0; 48000 * 3];
    for i in 0..sig_l.len() {
        let t = i as f32 / 48000.0;
        let voice_burst = if t % 1.5 < 1.0 { 1.0 } else { 0.0 };
        let voice_noise = ((t * 400.0 * 2.0 * PI).sin() * 0.1 
                         + (t * 800.0 * 2.0 * PI).sin() * 0.1 
                         + (t * 1500.0 * 2.0 * PI).sin() * 0.1) * voice_burst;
                   
        let bed = (t * 200.0 * 2.0 * PI).sin() * 0.1 + (t * 1000.0 * 2.0 * PI).sin() * 0.1;
        let sibilant_burst = if t % 1.5 > 0.8 && t % 1.5 < 1.0 { 1.0 } else { 0.0 };
        let sibilant = (t * 8000.0 * 2.0 * PI).sin() * sibilant_burst * 0.2;
        let hum = (t * 50.0 * 2.0 * PI).sin() * 0.1;
        let base_silence = (t * 6000.0 * 2.0 * PI).sin() * 0.0001;
        
        sig_l[i] = voice_noise + bed + sibilant + hum + base_silence;
        sig_r[i] = voice_noise + bed + sibilant + hum + base_silence;
    }
    let config = RestorationConfig::voice();
    render_triplet("combined", config, &sig_l, &sig_r, -45.0, 
        "everything cooperating, no artifacts");
}

fn get_configs() -> Vec<(&'static str, RestorationConfig)> {
    vec![
        ("gate", RestorationConfig { lowcut_enabled: false, hum_enabled: false, deess_enabled: false, gate_enabled: true }),
        ("deess", RestorationConfig { lowcut_enabled: false, hum_enabled: false, deess_enabled: true, gate_enabled: false }),
        ("lowcut", RestorationConfig { lowcut_enabled: true, hum_enabled: false, deess_enabled: false, gate_enabled: false }),
        ("dehum", RestorationConfig { lowcut_enabled: false, hum_enabled: true, deess_enabled: false, gate_enabled: false }),
        ("voice", RestorationConfig::voice()),
    ]
}

#[test]
#[ignore]
fn forensic_impulse_response() {
    for (name, config) in get_configs() {
        let mut sig_l = vec![0.0; 4096];
        let mut sig_r = vec![0.0; 4096];
        sig_l[0] = 1.0;
        sig_r[0] = 1.0;
        
        let mut chain = RestorationChain::new(48000.0, config.clone(), 0.0, -45.0);
        chain.process(&mut sig_l, &mut sig_r);
        
        write_wav(&format!("{}_ir.wav", name), &sig_l, &sig_r);
        let delay = measure_group_delay(config, -45.0);
        println!("{}_ir measured group delay: {} samples", name, delay);
    }
}

#[test]
#[ignore]
fn forensic_sine_sweep() {
    for (name, config) in get_configs() {
        let mut sig_l = vec![0.0; 48000 * 5];
        let mut sig_r = vec![0.0; 48000 * 5];
        
        let f0 = 20.0_f32;
        let f1 = 20000.0_f32;
        let big_t = 5.0_f32;
        
        for i in 0..sig_l.len() {
            let t = i as f32 / 48000.0;
            let phase = 2.0 * PI * f0 * big_t / (f1 / f0).ln() * ((f1 / f0).powf(t / big_t) - 1.0);
            
            sig_l[i] = phase.sin() * 0.5;
            sig_r[i] = phase.sin() * 0.5;
        }
        
        write_wav(&format!("{}_sweep_dry.wav", name), &sig_l, &sig_r);
        
        let mut wet_l = sig_l.clone();
        let mut wet_r = sig_r.clone();
        
        let mut chain = RestorationChain::new(48000.0, config, 0.0, -45.0);
        chain.process(&mut wet_l, &mut wet_r);
        
        write_wav(&format!("{}_sweep_wet.wav", name), &wet_l, &wet_r);
        
        println!("{}_sweep generated", name);
    }
}
