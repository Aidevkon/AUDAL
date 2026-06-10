//! E2E Album Quality — Ear Fatigue + ArcSwap assertions.
//! Proves INV-ALB-2: Single Source of Truth.
//! The same DspState that the Conductor commits is what
//! the audio thread reads via ArcSwap.

use arc_swap::ArcSwap;
use std::sync::Arc;
use xaak::repo::{AudioRepo, DspState};

#[test]
fn test_e2e_gapless_ear_fatigue_transition() {
    // 1. Initialize repo with flavours
    let initial_state = DspState::default();
    let mut repo      = AudioRepo::new_with_flavours(initial_state);

    // Clone the hot pointer BEFORE any commits
    // Simulates audio thread holding its own Arc reference
    let head_state_ptr = repo.head_state.clone();

    // 2. Mock track analyses (digital domain — dBFS, not SPL)
    // Track 1: Aggressive EDM
    //   peak:       -2.0 dBFS (very hot)
    //   integrated: -7.0 LUFS (loud)
    let track_1_integrated_lufs: f32 = -7.0;

    // Track 2: Soft acoustic — base state
    let mut track_2_dsp = DspState {
        ducking_depth:  0.9,
        sidechain_hold: 2,
        ms_width:       1.0,
        lfe_gain:       -1.0,
    };

    // 3. Psychoacoustic Ears Model
    // If Track 1 is louder than fatigue threshold → compensate Track 2
    // Threshold: -9.0 LUFS (anything louder triggers recovery)
    let fatigue_threshold_lufs: f32 = -9.0;
    if track_1_integrated_lufs > fatigue_threshold_lufs {
        // Recovery adjustments for Track 2 opening (~15 seconds)
        track_2_dsp.ducking_depth = 0.6; // softer transients
        track_2_dsp.ms_width      = 0.8; // narrower stereo field
    }

    // 4. Conductor commits adjusted state via AudioRepo
    // This simulates the zero-gap gapless transition
    repo.checkout("main").unwrap();
    repo.commit(track_2_dsp, "Ear fatigue recovery — Track 2 opening");

    // 5. Audio thread reads via ArcSwap (lock-free, O(1))
    let active_dsp = head_state_ptr.load_full();

    // 6. E2E Assertions — INV-ALB-2: Single Source of Truth
    assert!(
        (active_dsp.ducking_depth - 0.6).abs() < f32::EPSILON * 10.0,
        "Ear fatigue ducking not applied: expected 0.6, got {}",
        active_dsp.ducking_depth
    );
    assert!(
        (active_dsp.ms_width - 0.8).abs() < f32::EPSILON * 10.0,
        "Ear fatigue stereo narrowing not applied: expected 0.8, got {}",
        active_dsp.ms_width
    );
    assert!(
        active_dsp.sidechain_hold == 2,
        "Sidechain hold corrupted: expected 2, got {}",
        active_dsp.sidechain_hold
    );

    // 7. Prove ArcSwap isolation — original initial_state unchanged
    // Audio thread's Arc still sees the NEW committed state
    // (not the initial DspState::default())
    assert!(
        (active_dsp.ducking_depth - initial_state.ducking_depth).abs() > 0.1,
        "ArcSwap should reflect new state, not initial state"
    );

    println!("✅ INV-ALB-2 proven: Conductor commit = Audio thread read");
    println!("   ducking_depth: {} (fatigue recovery active)", active_dsp.ducking_depth);
    println!("   ms_width:      {} (stereo narrowed)", active_dsp.ms_width);
    println!("   Track 1 LUFS:  {} (above fatigue threshold {})",
        track_1_integrated_lufs, fatigue_threshold_lufs);
}
