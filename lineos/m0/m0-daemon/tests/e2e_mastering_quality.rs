//! E2E Mastering Quality Test
//! Validates the entire DspAdapter pipeline:
//! Input Audio -> LUFS Normalization -> Maestro AI Ducking -> True Peak Limiting -> Output LufsReport

use lineos_types::MasteringIntent;
use m0d::dsp::DspAdapter;

// Re-use our deterministic broadband generators inline for cross-crate test visibility
fn generate_techno_track(duration_s: f32, sample_rate: u32) -> (Vec<f32>, Vec<f32>) {
    let n = (duration_s * sample_rate as f32) as usize;
    let mut left = vec![0.0f32; n];
    let mut right = vec![0.0f32; n];
    
    for i in 0..n {
        let t = i as f32 / sample_rate as f32;
        // Kick (60Hz + click)
        let kick = (libm::sinf(2.0 * core::f32::consts::PI * 60.0 * t) * libm::expf(-t * 30.0) * 0.8) + 
                   (libm::sinf(2.0 * core::f32::consts::PI * 2000.0 * t) * libm::expf(-t * 200.0) * 0.3);
        // Sub Bass (50Hz)
        let bass = libm::sinf(2.0 * core::f32::consts::PI * 50.0 * t) * 0.6;
        
        let mix = (kick + bass).clamp(-1.0, 1.0);
        // Make it slightly quiet to force the autotuner to work (+ gain)
        left[i] = mix * 0.2;
        right[i] = mix * 0.2;
    }
    (left, right)
}

#[test]
fn e2e_pipeline_hits_lufs_and_true_peak() {
    let sample_rate = 48000;
    // 5 seconds of audio — enough for LUFS integration and NMF training
    let (mut left, mut right) = generate_techno_track(5.0, sample_rate);
    
    let target_lufs = -14.0;
    let target_tp = -1.0;
    
    // Use the built-in Spotify constructor (-14 LUFS, -1.0 TP)
    let mut intent = MasteringIntent::spotify();
    // Override the preset name for our specific test
    intent.preset_name = "techno_master".to_string();

    println!("Starting E2E Master (Intent: {} LUFS, {} TP)", target_lufs, target_tp);
    
    // EXECUTE THE CORE ENGINE
    let result = DspAdapter::master(
        &intent,
        &mut left,
        &mut right,
        sample_rate,
        None, // No manual Aether config overrides
    ).expect("DSP Pipeline failed to process the track");

    let final_lufs = result.lufs.integrated_lufs;
    let final_tp = result.lufs.true_peak_dbfs;

    println!("Final Integrated LUFS: {:.2} (Target: {:.2})", final_lufs, target_lufs);
    println!("Final True Peak:       {:.2} (Target: {:.2})", final_tp, target_tp);

    // Assert LUFS is within 0.5 LU of target
    assert!(
        (final_lufs - target_lufs).abs() <= 0.5,
        "Failed to hit target LUFS. Got {:.2}, Expected {:.2}", final_lufs, target_lufs
    );

    // Assert True Peak does not exceed ceiling (allow 0.1dB math tolerance)
    assert!(
        final_tp <= target_tp + 0.1,
        "True Peak exceeded ceiling! Got {:.2}, Ceiling {:.2}", final_tp, target_tp
    );
}
