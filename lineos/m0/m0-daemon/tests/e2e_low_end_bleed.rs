//! Low-End Bleed Test (S.2)
//! Proves that blind NMF fails to separate a Kick and Bass when they share the exact same frequency bin (50Hz).
//! EXPECTED TO FAIL until Low-Frequency Envelope Correlation is implemented.

use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::StftEngine;

// Inline generator for the test
fn kick_and_bass_clash(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let kick = libm::sinf(2.0 * core::f32::consts::PI * 50.0 * t) * libm::expf(-t * 40.0);
            let bass = if t > 0.1 {
                libm::sinf(2.0 * core::f32::consts::PI * 50.0 * t) * 0.5
            } else {
                0.0
            };
            (kick + bass).clamp(-1.0, 1.0)
        })
        .collect()
}

#[test]
fn test_kick_and_bass_separation() {
    let sample_rate = 48000;
    let signal = kick_and_bass_clash(1.0, sample_rate);

    let mut stft = StftEngine::new();
    let (frames, _) = stft.forward(&signal);

    let mag_frames: Vec<Vec<f32>> = frames
        .iter()
        .map(|frame| {
            frame
                .iter()
                .map(|c| (c.re * c.re + c.im * c.im).sqrt())
                .collect()
        })
        .collect();

    let mut nmf = NmfEngine::new(2); // Force into 2 components (ideally one for kick, one for bass)
    nmf.fit(&mag_frames);

    // Apply S.2 Low-End Routing
    nmf.resolve_low_end_clash();

    let n_frames = frames.len();

    // Check component 0 and 1 energy at t=0 (Kick Hit) and t=500ms (Bass Sustain)
    let hit_frame = 1;
    let sustain_frame = (0.5 * sample_rate as f32 / 1024.0) as usize;

    let c0_hit = nmf.h[hit_frame];
    let c0_sustain = nmf.h[sustain_frame];

    let c1_hit = nmf.h[n_frames + hit_frame];
    let c1_sustain = nmf.h[n_frames + sustain_frame];

    println!(
        "Component 0 -> Hit: {:.4}, Sustain: {:.4}",
        c0_hit, c0_sustain
    );
    println!(
        "Component 1 -> Hit: {:.4}, Sustain: {:.4}",
        c1_hit, c1_sustain
    );

    // If separation works, one component should hold the hit, and the OTHER should hold the sustain.
    // Blind NMF will put BOTH in the same component because they are both 50Hz.
    // We set a realistic noise-floor threshold of 10.0.
    // In STFT math, absolute 0.0 is rare due to windowing artifacts.
    let threshold = 10.0;
    let c0_has_both = c0_hit > threshold && c0_sustain > threshold;
    let c1_has_both = c1_hit > threshold && c1_sustain > threshold;

    assert!(
        !(c0_has_both || c1_has_both),
        "LOW-END BLEED DETECTED! A single component hoovered up both the Kick and the Bass. C0[hit:{:.2}, sus:{:.2}], C1[hit:{:.2}, sus:{:.2}]",
        c0_hit, c0_sustain, c1_hit, c1_sustain
    );
}
