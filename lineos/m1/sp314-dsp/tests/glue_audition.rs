use sp314_dsp::dsp::glue::{GlueChain, WidthMode};
use sp314_dsp::dsp::biquad::{butter_hp2_prewarped, butter_lp2_prewarped};
use sp314_dsp::limiter::core::{BrickwallLimiter, LimiterConfig};
use std::fs;

fn get_energies(l: &[f32], r: &[f32]) -> (f32, f32, f32, f32, f32) {
    let mut hpf_8k = butter_hp2_prewarped(8000.0, 48000.0);
    let mut lpf_150 = butter_lp2_prewarped(150.0, 48000.0);
    let mut hpf_8k_r = butter_hp2_prewarped(8000.0, 48000.0);
    let mut lpf_150_r = butter_lp2_prewarped(150.0, 48000.0);
    
    let mut e_hi = 0.0;
    let mut e_lo = 0.0;
    let mut sum_sq = 0.0;
    let mut side_sq = 0.0;
    let mut peak = 0.0f32;
    for (&sl, &sr) in l.iter().zip(r.iter()) {
        let hi_l = hpf_8k.process(sl);
        let lo_l = lpf_150.process(sl);
        let hi_r = hpf_8k_r.process(sr);
        let lo_r = lpf_150_r.process(sr);
        
        e_hi += hi_l * hi_l + hi_r * hi_r;
        e_lo += lo_l * lo_l + lo_r * lo_r;
        sum_sq += sl * sl + sr * sr;
        let side = (sl - sr) * 0.5;
        side_sq += side * side;
        if sl.abs() > peak { peak = sl.abs(); }
        if sr.abs() > peak { peak = sr.abs(); }
    }
    let rms = (sum_sq / (l.len() * 2) as f32).sqrt();
    let side_rms = (side_sq / l.len() as f32).sqrt();
    (rms, peak, side_rms, e_lo, e_hi)
}

#[ignore = "όργανο ΑΚΡΟΑΣΗΣ: θέλει το τοπικό MUSDB18HQ σε ΑΠΟΛΥΤΟ path /home/aidevcon/Downloads/DATASET/musdb18hq (:39) ⇒ περνάει ΜΟΝΟ σε αυτό το μηχάνημα, σε άλλο σκάει. Το ξυπνά: scripts/run-ignored.sh"]
#[test]
fn test_glue_audition() {
    let tracks = [
        ("/home/aidevcon/Downloads/DATASET/musdb18hq/test/Ben Carrigan - We'll Talk About It All Tonight/mixture.wav", "/tmp/glue_t2"),
        ("/home/aidevcon/Downloads/DATASET/musdb18hq/test/Hollow Ground - Ill Fate/mixture.wav", "/tmp/glue_t3"),
    ];

    for (wav_path, out_dir) in tracks.iter() {
        if !std::path::Path::new(wav_path).exists() {
            println!("Skipped: input not found {}", wav_path);
            continue;
        }
        
        fs::create_dir_all(out_dir).unwrap();
        
        let mut reader = hound::WavReader::open(wav_path).unwrap();
        let spec = reader.spec();
        let mut l_orig = Vec::new();
        let mut r_orig = Vec::new();
        
        if spec.sample_format == hound::SampleFormat::Float {
            let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
            for i in 0..(samples.len()/2) {
                l_orig.push(samples[2*i]);
                r_orig.push(samples[2*i+1]);
            }
        } else {
            let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
            for i in 0..(samples.len()/2) {
                l_orig.push(samples[2*i] as f32 / 32768.0);
                r_orig.push(samples[2*i+1] as f32 / 32768.0);
            }
        }
        
        let names = ["A_dry", "B_25", "C_50", "D_100", "E_100_drive"];
        
        let mut dry_mono_sq = 0.0;
        let mut dry_stereo_sq = 0.0;
        for (sl, sr) in l_orig.iter().zip(r_orig.iter()) {
            let m = (sl + sr) * 0.5;
            dry_mono_sq += m * m;
            dry_stereo_sq += sl * sl + sr * sr;
        }
        let dry_mono_rms = (dry_mono_sq / l_orig.len() as f32).sqrt().max(1e-12);
        let dry_stereo_rms = (dry_stereo_sq / (l_orig.len() * 2) as f32).sqrt().max(1e-12);

        println!("Track: {}", out_dir);
        println!("{:<15} | {:<7} | {:<7} | {:<8} | {:<12}", "name", "rms", "peak", "side_rms", "mono_fold_dB");
        println!("{}", "-".repeat(60));
        
        for (i, name) in names.iter().enumerate() {
            let mut l_wet = l_orig.clone();
            let mut r_wet = r_orig.clone();
            
            let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
            chain.set_amount(1.0);
            if i < 4 {
                chain.set_drive(0.0); // Width only
            }
            chain.process(&mut l_wet, &mut r_wet);
            
            let x = match i {
                0 => 0.0,
                1 => 0.25,
                2 => 0.50,
                3 => 1.0,
                4 => 1.0,
                _ => 0.0,
            };
            
            let mut l = l_orig.clone();
            let mut r = r_orig.clone();
            
            for j in 0..l.len() {
                l[j] = l_orig[j] + l_wet[j] * x;
                r[j] = r_orig[j] + r_wet[j] * x;
            }
            
            let cfg = LimiterConfig {
                release_ms: 15.0,
                blend_release_ms: 100.0,
                ceiling_db: -1.0,
                true_peak_enabled: true,
                midside_eq_enabled: false,
            };
            let mut lim = BrickwallLimiter::new(cfg, spec.sample_rate);
            lim.process_block(&mut l, &mut r);
            
            let (rms, peak, side_rms, _e_lo, _e_hi) = get_energies(&l, &r);
            
            let mut mono_sq = 0.0;
            for (sl, sr) in l.iter().zip(r.iter()) {
                let m = (sl + sr) * 0.5;
                mono_sq += m * m;
            }
            let mono_rms = (mono_sq / l.len() as f32).sqrt().max(1e-12);
            let mono_fold_db = 20.0 * mono_rms.log10() - 20.0 * dry_mono_rms.log10();
            
            println!("{:<15} | {:<7.4} | {:<7.4} | {:<8.4} | {:<12.4}", name, rms, peak, side_rms, mono_fold_db);
            
            let out_spec = hound::WavSpec {
                channels: 2,
                sample_rate: spec.sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut writer = hound::WavWriter::create(&format!("{}/{}.wav", out_dir, name), out_spec).unwrap();
            for (&sl, &sr) in l.iter().zip(r.iter()) {
                writer.write_sample(sl).unwrap();
                writer.write_sample(sr).unwrap();
            }
        }
        println!();
    }
}
