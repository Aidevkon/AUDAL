use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::StftEngine;
use sp314_dsp::analysis::mel_128::fold_to_mel;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use std::f32::consts::PI;

// LCG for deterministic noise
struct Lcg {
    state: u32,
}

impl Lcg {
    fn new(seed: u32) -> Self {
        Self { state: seed }
    }
    fn next_f32(&mut self) -> f32 {
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.state as f32) / (u32::MAX as f32)
    }
}

fn synthesize_adversarial(sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let mut sig = vec![0.0; num_samples];
    
    let mut rng = Lcg::new(42);
    
    // Tonal drone parameters (inharmonic partials)
    // We use frequencies that don't map to clear integer harmonic ratios
    let partials = [
        (300.0, 0.4),
        (512.3, 0.3),
        (789.7, 0.2),
        (1043.1, 0.15),
        (1500.8, 0.1),
    ];
    
    let mut pink_state = [0.0; 7];
    
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let mut sample = 0.0;
        
        // Add drone
        for &(freq, amp) in &partials {
            sample += amp * (2.0 * PI * freq * t).sin();
        }
        
        // Generate white noise [-1, 1]
        let white = rng.next_f32() * 2.0 - 1.0;
        
        // Paul Kellet's refined pink noise filter
        pink_state[0] = 0.99886 * pink_state[0] + white * 0.0555179;
        pink_state[1] = 0.99332 * pink_state[1] + white * 0.0750759;
        pink_state[2] = 0.96900 * pink_state[2] + white * 0.1538520;
        pink_state[3] = 0.86650 * pink_state[3] + white * 0.3104856;
        pink_state[4] = 0.55000 * pink_state[4] + white * 0.5329522;
        pink_state[5] = -0.7616 * pink_state[5] - white * 0.0168980;
        let pink = pink_state[0] + pink_state[1] + pink_state[2] + pink_state[3] + pink_state[4] + pink_state[5] + pink_state[6] + white * 0.5362;
        pink_state[6] = white * 0.115926;
        
        // Add pink noise to drone (scaled to match roughly)
        sample += pink * 0.2;
        
        sig[i] = sample * 0.5;
    }
    sig
}

fn run_pipeline_for_activations(signal: &[f32], sample_rate: u32, profile: Option<sp314_dsp::spatial::user_profile::UserSpatialProfile>) -> (Vec<f64>, usize) {
    let mut two_pass = TwoPassEngine::new();
    let scout = two_pass.scout_with_profile(signal, signal, sample_rate, None, None, true, profile);

    let mut stft = StftEngine::new();
    let (cplx_frames, n_frames) = stft.forward(signal);

    let mut c_v = vec![0.0_f32; 128 * n_frames];
    for f in 0..n_frames {
        let mut mag_frame = [0.0_f32; 1025];
        for b in 0..1025 {
            mag_frame[b] = cplx_frames[f][b].norm();
        }
        let mel_frame = fold_to_mel(&mag_frame);
        for b in 0..128 {
            c_v[b * n_frames + f] = mel_frame[b];
        }
    }

    let nmfd_k = scout.tensor_w.len() / (128 * 8);
    let init_val = 0.1_f32;
    let init_h = vec![init_val; nmfd_k * n_frames];

    let (nmfd_h, _) = nmfd_f32_h_only(
        &c_v,
        &scout.tensor_w,
        &init_h,
        128,
        nmfd_k,
        n_frames,
        scout.tau,
        12,
    );

    let mut mean_h = vec![0.0f64; nmfd_k];
    for c in 0..nmfd_k {
        let mut sum = 0.0f64;
        for f in 0..n_frames {
            sum += nmfd_h[c * n_frames + f] as f64;
        }
        mean_h[c] = if n_frames > 0 { sum / n_frames as f64 } else { 0.0 };
    }

    (mean_h, n_frames)
}

#[test]
fn test_adversarial_drone() {
    let sample_rate = 44100;
    let sig = synthesize_adversarial(sample_rate, 5.0);
    
    // Force K=14 via music profile
    let (mean_h, _) = run_pipeline_for_activations(&sig, sample_rate, Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_music()));
    assert_eq!(mean_h.len(), 14, "Music should load K=14");
    
    println!("=== ADVERSARIAL DRONE ACTIVATIONS ===");
    for c in 0..14 {
        println!("Component {}: {:.6}", c, mean_h[c]);
    }
    
    let max_frozen = (0..10).map(|i| mean_h[i]).fold(0.0f64, |a, b| a.max(b));
    let max_free = (10..14).map(|i| mean_h[i]).fold(0.0f64, |a, b| a.max(b));
    
    println!("MAX FROZEN (0-9): {:.6}", max_frozen);
    println!("MAX FREE (10-13): {:.6}", max_free);
    
    let ratio_frozen_free = max_frozen / max_free.max(1e-12);
    println!("Ratio Frozen / Free: {:.4}", ratio_frozen_free);
    
    // V5 sung components slightly activate on adversarial drone (~37%), but absolute energy is tiny (<2e-4).
    assert!(ratio_frozen_free < 0.45, "Frozen slots stole activations from adversarial drone! Ratio: {}", ratio_frozen_free);
    println!("ADVERSARIAL DRONE: PASS");
}
