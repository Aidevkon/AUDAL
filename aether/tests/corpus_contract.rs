use lineos_corpus::contract::*;
use lineos_corpus::builder::build_timeline;
use lineos_types::analysis::{StemFeatures, StemMetrics};
use lineos_types::pre_analysis::PreAnalysisData;

fn mock_features() -> StemFeatures {
    StemFeatures {
        voice:     StemMetrics { rms_db: -18.0, crest_factor_db: 12.0, ..Default::default() },
        drums:     StemMetrics { rms_db: -12.0, crest_factor_db: 18.0, ..Default::default() },
        bass:      StemMetrics { rms_db: -20.0, crest_factor_db: 6.0,  ..Default::default() },
        harmonics: StemMetrics { rms_db: -22.0, crest_factor_db: 8.0,  ..Default::default() },
        ambience:  StemMetrics { rms_db: -30.0, crest_factor_db: 4.0,  ..Default::default() },
        mix:       lineos_types::analysis::MixMetrics::default(),
    }
}

#[test]
fn corpus_protocol_version_is_900() {
    let env = build_timeline(&mock_features(), &PreAnalysisData::silent(), "test-blob-id", 5000, "broadcast");
    assert_eq!(env.protocol_version, "900");
}

#[test]
fn corpus_session_id_matches_blob_id() {
    let blob_id = "abc123-test";
    let env = build_timeline(&mock_features(), &PreAnalysisData::silent(), blob_id, 5000, "broadcast");
    assert_eq!(env.session_id, blob_id);
}

#[test]
fn corpus_has_all_five_stems() {
    let env = build_timeline(&mock_features(), &PreAnalysisData::silent(), "test", 5000, "broadcast");
    let stem_types: Vec<&str> = env.stems.iter().map(|s| s.stem_type.as_str()).collect();
    assert!(stem_types.contains(&"voice"));
    assert!(stem_types.contains(&"drums"));
    assert!(stem_types.contains(&"bass"));
    assert!(stem_types.contains(&"harmonics"));
    assert!(stem_types.contains(&"ambience"));
}

#[test]
fn corpus_events_have_session_id_guard() {
    // INV-CP-9: every event carries session_id
    let blob_id = "guard-test";
    let env = build_timeline(&mock_features(), &PreAnalysisData::silent(), blob_id, 5000, "broadcast");
    for stem in &env.stems {
        for event in &stem.events {
            assert_eq!(event.session_id, blob_id, "INV-CP-9 violated");
        }
    }
}

#[test]
fn corpus_confidence_within_bounds() {
    // INV-CP-6: confidence in [0.0, 1.0]
    let env = build_timeline(&mock_features(), &PreAnalysisData::silent(), "test", 5000, "broadcast");
    for stem in &env.stems {
        for event in &stem.events {
            assert!(event.confidence >= 0.0 && event.confidence <= 1.0);
        }
    }
}

#[test]
fn corpus_no_audio_content() {
    // Zero audio — only behavioral stats
    let env = build_timeline(&mock_features(), &PreAnalysisData::silent(), "test", 5000, "broadcast");
    let json = serde_json::to_string(&env).unwrap();
    // No waveform data — just numbers
    assert!(!json.contains("waveform"));
    assert!(!json.contains("samples"));
    assert!(!json.contains("pcm"));
}

#[test]
fn corpus_deterministic() {
    // INV-CP-2: same input → same output
    let f = mock_features();
    let p = PreAnalysisData::silent();
    let e1 = build_timeline(&f, &p, "det-test", 5000, "broadcast");
    let e2 = build_timeline(&f, &p, "det-test", 5000, "broadcast");
    assert_eq!(serde_json::to_string(&e1).unwrap(),
               serde_json::to_string(&e2).unwrap());
}
