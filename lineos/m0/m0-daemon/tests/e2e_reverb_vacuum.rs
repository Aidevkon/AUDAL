//! Reverb Vacuum Effect Test
//! Proves that blind NMF leaks the reverb tail of a snare into the Ambience component.
//! EXPECTED TO FAIL until ExponentialDecayBinder is implemented.

use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::StftEngine;

fn snare_with_reverb(duration_s: f32, sample_rate: u32, seed: u64) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let mut state = seed;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((state >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let hit = noise * libm::expf(-t * 150.0);
            let tail = noise * libm::expf(-t * 6.9) * 0.5;
            (hit + tail).clamp(-1.0, 1.0)
        })
        .collect()
}

#[test]
fn test_reverb_decay_stays_in_drums() {
    let sample_rate = 48000;
    let duration = 1.0;
    let snare = snare_with_reverb(duration, sample_rate, 42);

    let mut stft = StftEngine::new();
    let (frames, _) = stft.forward(&snare);

    let mag_frames: Vec<Vec<f32>> = frames
        .iter()
        .map(|frame| {
            frame
                .iter()
                .map(|c| (c.re * c.re + c.im * c.im).sqrt())
                .collect()
        })
        .collect();

    let mut nmf = NmfEngine::new(5);
    nmf.fit(&mag_frames);

    let n_frames = frames.len();
    let n_bins = 1024;

    let mut max_transient_idx = 0;
    let mut max_transient_val = 0.0;
    for c in 0..5 {
        let energy_at_hit = nmf.h[c * n_frames + 1];
        if energy_at_hit > max_transient_val {
            max_transient_val = energy_at_hit;
            max_transient_idx = c;
        }
    }

    // --- RENDER PIPELINE ---
    // Generate the raw mask for the Drums component
    let mut drum_mask = nmf.component_mask_chunk(max_transient_idx, &nmf.h, n_frames, n_bins);

    let tail_frame = (0.3 * sample_rate as f32 / 1024.0) as usize;

    // Measure raw mask state at 300ms (Vacuum Effect active)
    let raw_tail_avg: f32 = drum_mask[tail_frame].iter().sum::<f32>() / n_bins as f32;

    // Apply the S.4 Exponential Decay Binder
    sp314_dsp::stft::stem_renderer::refine_mask(&mut drum_mask);

    // Measure refined mask state at 300ms (Vacuum Effect defeated)
    let refined_tail_avg: f32 = drum_mask[tail_frame].iter().sum::<f32>() / n_bins as f32;

    println!("Raw Mask @ 300ms:     {:.4}", raw_tail_avg);
    println!("Refined Mask @ 300ms: {:.4}", refined_tail_avg);

    // The raw mask will be completely closed (~0.0) due to NMF amnesia.
    // The refined mask should be held open (>0.1) by the exponential physical floor.
    assert!(
        refined_tail_avg > 0.1,
        "Decay binder failed! Refined mask collapsed to: {:.4}",
        refined_tail_avg
    );

    // Ensure the binder actually made a difference
    assert!(
        refined_tail_avg > raw_tail_avg * 5.0,
        "Binder did not significantly alter the mask. Refined: {:.4}, Raw: {:.4}",
        refined_tail_avg,
        raw_tail_avg
    );
}
