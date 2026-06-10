//! Formant Clash Bleed Test (S.3)
//! Proves that blind NMF groups a static Synth and a vibrating Human Voice together if they share the same fundamental frequency.

use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::StftEngine;

// Inline generator
fn vocal_and_synth_clash(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let mut phase_static = 0.0_f32;
    let mut phase_vib = 0.0_f32;
    (0..n).map(|i| {
        let t = i as f32 / sample_rate as f32;
        let synth_env = if t > 0.1 && t < 0.4 { 1.0 } else { 0.0 };
        phase_static += 2.0 * core::f32::consts::PI * 440.0 / sample_rate as f32;
        let synth = libm::sinf(phase_static) * synth_env;
        
        let vib_env = if t > 0.6 && t < 0.9 { 1.0 } else { 0.0 };
        let current_freq = 440.0 + 15.0 * libm::sinf(2.0 * core::f32::consts::PI * 6.0 * t);
        phase_vib += 2.0 * core::f32::consts::PI * current_freq / sample_rate as f32;
        let vocal = libm::sinf(phase_vib) * vib_env;
        (synth + vocal).clamp(-1.0, 1.0)
    }).collect()
}

#[test]
fn test_formant_clash_separation() {
    let sample_rate = 48000;
    let signal = vocal_and_synth_clash(1.0, sample_rate);

    let mut stft = StftEngine::new();
    let (frames, _) = stft.forward(&signal);
    
    let mag_frames: Vec<Vec<f32>> = frames.iter()
        .map(|frame| frame.iter().map(|c| (c.re * c.re + c.im * c.im).sqrt()).collect())
        .collect();

    let mut nmf = NmfEngine::new(2);
    nmf.fit(&mag_frames);
    
    // Apply S.3 Mid-Range Routing
    nmf.resolve_formant_clash();

    let n_frames = frames.len();
    
    // STFT Hop Size is 512
    let synth_frame_center = (0.250 * sample_rate as f32 / 512.0) as usize;
    let vocal_frame_center = (0.750 * sample_rate as f32 / 512.0) as usize;

    let get_peak = |c: usize, center: usize| -> f32 {
        let start = center.saturating_sub(2);
        let end = (center + 3).min(n_frames);
        let mut peak = 0.0_f32;
        for f in start..end {
            let val = nmf.h[c * n_frames + f];
            if val > peak { peak = val; }
        }
        peak
    };

    let c0_synth = get_peak(0, synth_frame_center);
    let c0_vocal = get_peak(0, vocal_frame_center);
    
    let c1_synth = get_peak(1, synth_frame_center);
    let c1_vocal = get_peak(1, vocal_frame_center);

    println!("Component 0 -> Synth: {:.4}, Vocal: {:.4}", c0_synth, c0_vocal);
    println!("Component 1 -> Synth: {:.4}, Vocal: {:.4}", c1_synth, c1_vocal);

    let threshold = 50.0; 
    let c0_has_both = c0_synth > threshold && c0_vocal > threshold;
    let c1_has_both = c1_synth > threshold && c1_vocal > threshold;

    assert!(
        !(c0_has_both || c1_has_both),
        "FORMANT CLASH BLEED DETECTED! A single component hoovered up both the Synth and the Vocal. C0[synth:{:.2}, voc:{:.2}], C1[synth:{:.2}, voc:{:.2}]",
        c0_synth, c0_vocal, c1_synth, c1_vocal
    );
}
