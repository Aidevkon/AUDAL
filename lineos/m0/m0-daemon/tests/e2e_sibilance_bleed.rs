//! Sibilance Bleed Test (S.1)
//! Proves that blind NMF groups Drum Hi-Hats and Vocal Sibilance together due to identical spectral noise profiles.
//! EXPECTED TO FAIL until Morphological Component Analysis (Length-based routing) is implemented.

use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::StftEngine;

fn hihat_and_sibilance_clash(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let mut state = 42u32;
    (0..n).map(|i| {
        let t = i as f32 / sample_rate as f32;
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let hat_env = if t > 0.1 && t < 0.15 { libm::expf(-(t - 0.1) * 100.0) } else { 0.0 };
        let sib_env = if t > 0.4 && t < 0.55 { libm::expf(-(t - 0.4) * 20.0) } else { 0.0 };
        (noise * hat_env + noise * sib_env * 0.8).clamp(-1.0, 1.0)
    }).collect()
}

#[test]
fn test_sibilance_separation() {
    let sample_rate = 48000;
    let signal = hihat_and_sibilance_clash(1.0, sample_rate);
    let peak_signal = signal.iter().cloned().fold(0.0_f32, f32::max);
    println!("DEBUG: Max signal amplitude generated: {:.4}", peak_signal);

    let mut stft = StftEngine::new();
    let (frames, _) = stft.forward(&signal);
    
    let mag_frames: Vec<Vec<f32>> = frames.iter()
        .map(|frame| frame.iter().map(|c| (c.re * c.re + c.im * c.im).sqrt()).collect())
        .collect();

    let mut nmf = NmfEngine::new(2);
    nmf.fit(&mag_frames);
    
    // Apply S.1 High-End Routing
    nmf.resolve_high_end_clash();

    let n_frames = mag_frames.len();
    
    // STFT Hop Size is 512!
    let hat_frame_center = (0.105 * sample_rate as f32 / 512.0) as usize;
    let sib_frame_center = (0.410 * sample_rate as f32 / 512.0) as usize;

    // Helper to find peak energy in a window (±2 frames to be perfectly safe)
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

    let c0_hat = get_peak(0, hat_frame_center);
    let c0_sib = get_peak(0, sib_frame_center);
    
    let c1_hat = get_peak(1, hat_frame_center);
    let c1_sib = get_peak(1, sib_frame_center);

    println!("Component 0 -> Hat: {:.4}, Sibilance: {:.4}", c0_hat, c0_sib);
    println!("Component 1 -> Hat: {:.4}, Sibilance: {:.4}", c1_hat, c1_sib);

    // With peaks hitting 6000-8000, a threshold of 1000 is a rock-solid noise floor check
    let threshold = 1000.0; 
    let c0_has_both = c0_hat > threshold && c0_sib > threshold;
    let c1_has_both = c1_hat > threshold && c1_sib > threshold;

    assert!(
        !(c0_has_both || c1_has_both),
        "SIBILANCE BLEED DETECTED! A single component stole both the Hi-Hat and the Vocal Sibilance. C0[hat:{:.2}, sib:{:.2}], C1[hat:{:.2}, sib:{:.2}]",
        c0_hat, c0_sib, c1_hat, c1_sib
    );
}
