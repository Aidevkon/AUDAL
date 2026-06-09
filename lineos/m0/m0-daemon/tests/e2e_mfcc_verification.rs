//! E2E MFCC Verification Tests
//! Proves:
//! 1. MfccAnalyzer runs and produces non-zero coefficients
//! 2. Different frequencies produce different MFCCs
//! 3. UserMarkovModel accumulates sessions incrementally

use lineos_corpus::mfcc::MfccAnalyzer;
use lineos_corpus::store::UserMarkovModel;
use lineos_corpus::contract::{
    CorpusEnvelope, StemTimeline, TimelineEvent,
    EnrichedAttributes, RiskFlags, DomainHint,
};
use std::f32::consts::PI;

fn sine_signal(freq_hz: f32, duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    (0..n).map(|i| {
        let t = i as f32 / sample_rate as f32;
        libm::sinf(2.0 * PI * freq_hz * t) * 0.5
    }).collect()
}

fn make_session(preset_id: &str, stem: &str) -> CorpusEnvelope {
    let event = TimelineEvent {
        state:       "sustain".to_string(),
        start_ms:    0,
        end_ms:      100,
        duration_ms: 100,
        confidence:  0.9,
        session_id:  "test".to_string(),
        attributes:  EnrichedAttributes {
            rms_db:            -20.0,
            crest_factor_db:   6.0,
            transient_density: 0.1,
            spectral_centroid: 2000.0,
            lufs_integrated:   -14.0,
            spectral_flatness: 0.3,
        },
        risk: RiskFlags {
            artifact_risk: 0.0, sibilance_risk: 0.0,
            phase_issue:   0.0, sub_rumble:     0.0,
        },
        domain: DomainHint {
            stem:         stem.to_string(),
            profile_hint: preset_id.to_string(),
        },
        mfcc: [0.0f32; 13],
    };
    CorpusEnvelope {
        protocol_version: "900".to_string(),
        session_id:       "test-session".to_string(),
        stems: vec![StemTimeline {
            stem_type: stem.to_string(),
            events:    vec![event],
        }],
    }
}

/// Test 1: MfccAnalyzer produces non-zero coefficients for real audio
#[test]
fn mfcc_analyzer_produces_nonzero_output() {
    let mut analyzer = MfccAnalyzer::new();

    // 440Hz sine — musical note A4
    let signal = sine_signal(440.0, 0.1, 48000);
    let mfcc = analyzer.compute(&signal);

    // Must not be all zeros
    let all_zero = mfcc.iter().all(|&x| x == 0.0);
    assert!(!all_zero, "MFCCs must be non-zero for real audio signal");

    // All must be finite
    for (i, &c) in mfcc.iter().enumerate() {
        assert!(c.is_finite(), "MFCC[{}] = {} is not finite", i, c);
    }
}

/// Test 2: Different frequencies produce different MFCC fingerprints
#[test]
fn mfcc_distinguishes_bass_from_treble() {
    let mut analyzer = MfccAnalyzer::new();

    // Sub-bass (808 kick range)
    let bass   = sine_signal(50.0,   0.1, 48000);
    // Hi-hat range
    let treble = sine_signal(8000.0, 0.1, 48000);

    let mfcc_bass   = analyzer.compute(&bass);
    let mfcc_treble = analyzer.compute(&treble);

    // L2 distance between the two fingerprints
    let distance: f32 = mfcc_bass.iter()
        .zip(mfcc_treble.iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt();

    assert!(
        distance > 1.0,
        "Bass (50Hz) and treble (8kHz) must have distinct MFCC fingerprints. \
         Got distance={:.3}", distance
    );
    println!("Bass vs Treble MFCC distance: {:.3}", distance);
}

/// Test 3: UserMarkovModel accumulates sessions incrementally
#[test]
fn markov_model_incremental_learning() {
    let mut model = UserMarkovModel::new("test_user");

    let session1 = make_session("spotify", "voice");
    let session2 = make_session("spotify", "voice");

    model.update("spotify", &session1);
    assert_eq!(model.version, 2, "Version must increment after first update");

    model.update("spotify", &session2);
    assert_eq!(model.version, 3, "Version must increment after second update");

    let sessions = model.preset("spotify")
        .and_then(|p| p.stem("voice"))
        .map(|s| s.n_sessions)
        .unwrap_or(0);
    assert_eq!(sessions, 2,
        "Voice model must have 2 sessions after 2 updates");

    // Serialize and deserialize — model survives round-trip
    let json = model.to_json().expect("Serialize failed");
    let restored = UserMarkovModel::from_json(&json)
        .expect("Deserialize failed");
    assert_eq!(restored.version, 3);
    assert_eq!(
        restored.preset("spotify")
            .and_then(|p| p.stem("voice"))
            .map(|s| s.n_sessions)
            .unwrap_or(0),
        2
    );
    println!("UserMarkovModel JSON size: {} bytes", json.len());
}
