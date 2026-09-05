use sp314_dsp::dsp::glue::{GlueChain, WidthMode};
use sp314_dsp::dsp::biquad::{butter_hp2_prewarped, butter_lp2_prewarped, Biquad};
use std::f32::consts::PI;

#[ignore = "όργανο χαρακτηρισμού (τυπώνει καμπύλη THD, δεν κρίνει)· θέλει /tmp/blue/nmf5/ambience.wav (:86, F-072). Δηλωμένη ΕΞΑΙΡΕΣΗ στο scripts/run-ignored.sh."]
#[test]
fn test_glue_characterize() {
    println!("=== 1. THD CURVE ===");
    println!("amount | THD%   | rms_out/rms_in dB");
    for &amount in &[0.0, 0.25, 0.5, 0.75, 1.0] {
        let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
        chain.set_amount(amount);
        
        let mut l = vec![0.0; 48000];
        let mut r = vec![0.0; 48000];
        for i in 0..48000 {
            let t = i as f32 / 48000.0;
            let val = 0.5 * (2.0 * PI * 1000.0 * t).sin();
            l[i] = val;
            r[i] = val;
        }
        
        let rms_in = (l.iter().map(|&x| x*x).sum::<f32>() / 48000.0).sqrt();
        
        chain.process(&mut l, &mut r);
        
        let rms_out = (l.iter().map(|&x| x*x).sum::<f32>() / 48000.0).sqrt();
        let db_gain = 20.0 * (rms_out / rms_in).log10();
        
        let mut e_fund = 0.0;
        let mut e_harm = 0.0;
        for &freq in &[1000.0, 2000.0, 3000.0, 4000.0, 5000.0] {
            let mut re = 0.0;
            let mut im = 0.0;
            for (i, &s) in l.iter().enumerate() {
                let t = i as f32 / 48000.0;
                let angle = 2.0 * PI * freq * t;
                re += s * angle.cos();
                im -= s * angle.sin();
            }
            let energy = re * re + im * im;
            if freq == 1000.0 {
                e_fund = energy;
            } else {
                e_harm += energy;
            }
        }
        
        let thd = if e_fund > 0.0 { (e_harm / e_fund).sqrt() * 100.0 } else { 0.0 };
        println!("{:<6} | {:<6.2}% | {:+.2} dB", amount, thd, db_gain);
    }
    
    println!("\n=== 2. WIDTH CURVE ===");
    println!("amount | width_gain_dB | mono_sum_dB");
    for &amount in &[0.0, 0.25, 0.5, 0.75, 1.0] {
        let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
        chain.set_amount(amount);
        
        let mut l = vec![0.0; 48000];
        let mut r = vec![0.0; 48000];
        for i in 0..48000 {
            let t = i as f32 / 48000.0;
            l[i] = 0.5 * (2.0 * PI * 1000.0 * t).sin();
            r[i] = 0.3 * l[i];
        }
        
        let s_in_rms = (l.iter().zip(r.iter()).map(|(&ll, &rr)| (ll - rr)*0.5).map(|x| x*x).sum::<f32>() / 48000.0).sqrt();
        let m_in_rms = (l.iter().zip(r.iter()).map(|(&ll, &rr)| (ll + rr)*0.5).map(|x| x*x).sum::<f32>() / 48000.0).sqrt();
        let in_ratio = s_in_rms / m_in_rms.max(1e-9);
        let in_mono = m_in_rms;
        
        chain.process(&mut l, &mut r);
        
        let s_out_rms = (l.iter().zip(r.iter()).map(|(&ll, &rr)| (ll - rr)*0.5).map(|x| x*x).sum::<f32>() / 48000.0).sqrt();
        let m_out_rms = (l.iter().zip(r.iter()).map(|(&ll, &rr)| (ll + rr)*0.5).map(|x| x*x).sum::<f32>() / 48000.0).sqrt();
        let out_ratio = s_out_rms / m_out_rms.max(1e-9);
        let out_mono = m_out_rms;
        
        let width_gain = 20.0 * (out_ratio / in_ratio.max(1e-9)).log10();
        let mono_gain = 20.0 * (out_mono / in_mono.max(1e-9)).log10();
        
        println!("{:<6} | {:+13.2} | {:+.2} dB", amount, width_gain, mono_gain);
    }
    
    println!("\n=== 3. REAL MATERIAL (amount=0) ===");
    let wav_path = "/tmp/blue/nmf5/ambience.wav";
    let mut reader = hound::WavReader::open(wav_path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    };
    
    let mut l = samples.clone();
    let mut r = samples.clone();
    
    let rms_in = (l.iter().map(|&x| x*x).sum::<f32>() / l.len() as f32).sqrt();
    
    // We can't access Biquad::process if it requires mutable reference and we don't have it imported right?
    // Wait, butter_hp2_prewarped returns Biquad, which has process(&mut self, x: f32) -> f32.
    // Let's assume Biquad is public.
    let mut hpf_8k = butter_hp2_prewarped(8000.0, 48000.0);
    let mut lpf_150 = butter_lp2_prewarped(150.0, 48000.0);
    
    let mut e_hi_in = 0.0;
    let mut e_lo_in = 0.0;
    for &s in &l {
        let hi = hpf_8k.process(s);
        let lo = lpf_150.process(s);
        e_hi_in += hi * hi;
        e_lo_in += lo * lo;
    }
    
    let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
    chain.set_amount(0.0);
    chain.process(&mut l, &mut r);
    
    let rms_out = (l.iter().map(|&x| x*x).sum::<f32>() / l.len() as f32).sqrt();
    let db_diff = 20.0 * (rms_out / rms_in.max(1e-9)).log10();
    
    let mut hpf_8k_2 = butter_hp2_prewarped(8000.0, 48000.0);
    let mut lpf_150_2 = butter_lp2_prewarped(150.0, 48000.0);
    let mut e_hi_out = 0.0;
    let mut e_lo_out = 0.0;
    for &s in &l {
        let hi = hpf_8k_2.process(s);
        let lo = lpf_150_2.process(s);
        e_hi_out += hi * hi;
        e_lo_out += lo * lo;
    }
    
    println!("rms_in : {}", rms_in);
    println!("rms_out: {}", rms_out);
    println!("diff dB: {:.2}", db_diff);
    println!("Energy > 8kHz : in={:.6e}, out={:.6e} (ratio={:.2} dB)", e_hi_in, e_hi_out, 10.0 * (e_hi_out/e_hi_in.max(1e-9)).log10());
    println!("Energy < 150Hz: in={:.6e}, out={:.6e} (ratio={:.2} dB)", e_lo_in, e_lo_out, 10.0 * (e_lo_out/e_lo_in.max(1e-9)).log10());
}
