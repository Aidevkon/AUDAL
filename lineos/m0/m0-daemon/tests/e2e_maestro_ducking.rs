//! Maestro Engine E2E Test
//! Proves that without Smart Ducking (Sidechain), the Bass maintains full volume during a Kick hit, causing a headroom clash.

use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::StftEngine;

fn clean_kick_and_bass(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let mut phase_bass = 0.0_f32;
    (0..n).map(|i| {
        let t = i as f32 / sample_rate as f32;
        let kick = if t >= 0.1 { libm::sinf(2.0 * core::f32::consts::PI * 50.0 * (t - 0.1)) * libm::expf(-(t - 0.1) * 40.0) } else { 0.0 };
        phase_bass += 2.0 * core::f32::consts::PI * 150.0 / sample_rate as f32;
        let bass = libm::sinf(phase_bass) * 0.5;
        (kick + bass).clamp(-1.0, 1.0)
    }).collect()
}

#[test]
fn test_maestro_smart_ducking() {
    let sample_rate = 48000;
    let signal = clean_kick_and_bass(1.0, sample_rate);

    let mut stft = StftEngine::new();
    let (frames, _) = stft.forward(&signal);
    
    let mag_frames: Vec<Vec<f32>> = frames.iter()
        .map(|frame| frame.iter().map(|c| (c.re * c.re + c.im * c.im).sqrt()).collect())
        .collect();

    let mut nmf = NmfEngine::new(2);
    nmf.fit(&mag_frames);

    // 1. CLEAN THE STEMS: Enforce physical laws to separate transients from sustain
    nmf.resolve_low_end_clash();

    let n_frames = frames.len();
    
    // Kick is a transient, Bass is sustained.
    // Correct Row-Major slice summation
    let energy_c0: f32 = nmf.h[0..n_frames].iter().sum();
    let energy_c1: f32 = nmf.h[n_frames..2 * n_frames].iter().sum();
    
    // Bass has more total energy over 1s because it is sustained
    let (kick_c, bass_c) = if energy_c1 > energy_c0 { 
        (0, 1)
    } else { 
        (1, 0)
    };

    // Apply Maestro Smart Ducking (Sidechain Bass to Kick)
    nmf.apply_smart_ducking(kick_c, bass_c);

    // Kick hits at 0.1s (STFT hop is 512)
    let hit_frame = (0.105 * sample_rate as f32 / 512.0) as usize;
    let sustain_frame = (0.500 * sample_rate as f32 / 512.0) as usize;

    let bass_at_hit = nmf.h[bass_c * n_frames + hit_frame];
    let bass_sustain = nmf.h[bass_c * n_frames + sustain_frame];

    println!("Bass Sustain Volume: {:.4}", bass_sustain);
    println!("Bass Volume during Kick Hit: {:.4}", bass_at_hit);

    // We want the Maestro engine to "duck" the bass by at least 50% when the kick hits.
    // If there is no ducking, bass_at_hit will be roughly equal to bass_sustain.
    let ducking_target = bass_sustain * 0.5;

    assert!(
        bass_at_hit < ducking_target,
        "MAESTRO DUCKING FAILED! The Bass did not duck for the Kick. Sustain: {:.2}, At Hit: {:.2}",
        bass_sustain, bass_at_hit
    );
}
