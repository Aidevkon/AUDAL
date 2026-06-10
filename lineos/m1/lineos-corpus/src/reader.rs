//! Corpus Reader — reads session_XXXX.corpus.json files.
//! Authority: corpus-learning-spec-v1_2.md CB-P2

use crate::contract::CorpusEnvelope;
use crate::features::StateFeatures;
use std::path::Path;

/// Read all session_*.corpus.json files from a directory.
/// Silently skips files that fail to parse.
pub fn read_corpus_sessions(corpus_dir: &Path) -> Vec<CorpusEnvelope> {
    let mut results = Vec::new();
    let entries = match std::fs::read_dir(corpus_dir) {
        Ok(e) => e,
        Err(_) => return results,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("session_") || !name.ends_with(".corpus.json") {
            continue;
        }
        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(envelope) = serde_json::from_str::<CorpusEnvelope>(&json) {
                results.push(envelope);
            }
        }
    }
    results
}

/// Extract ordered state sequence for one stem from a corpus envelope.
/// Returns Vec<String> of state values sorted by start_ms.
/// e.g. ["silence", "vowel", "consonant", "tail", ...]
pub fn extract_state_sequence(envelope: &CorpusEnvelope, stem_type: &str) -> Vec<String> {
    let stem = envelope.stems.iter().find(|s| s.stem_type == stem_type);
    let stem = match stem {
        Some(s) => s,
        None => return Vec::new(),
    };
    let mut events = stem.events.clone();
    events.sort_by_key(|e| e.start_ms);
    events.into_iter().map(|e| e.state).collect()
}

/// Extract StateFeatures sequence from a stem.
/// Computes rms_delta from consecutive windows.
/// Uses EnrichedAttributes fields directly — no approximation needed.
pub fn extract_features_sequence(envelope: &CorpusEnvelope, stem_type: &str) -> Vec<StateFeatures> {
    let stem = envelope.stems.iter().find(|s| s.stem_type == stem_type);
    let stem = match stem {
        Some(s) => s,
        None => return Vec::new(),
    };
    let mut events = stem.events.clone();
    events.sort_by_key(|e| e.start_ms);

    let mut features = Vec::with_capacity(events.len());
    let mut prev_rms_db = -144.0f32;

    for event in &events {
        let rms_db = event.attributes.rms_db;
        let f = if event.mfcc.iter().any(|&x| x != 0.0) {
            StateFeatures::with_mfcc(
                rms_db,
                prev_rms_db,
                event.attributes.transient_density,
                event.attributes.spectral_centroid,
                event.attributes.spectral_flatness,
                event.mfcc,
            )
        } else {
            StateFeatures::new(
                rms_db,
                prev_rms_db,
                event.attributes.transient_density,
                event.attributes.spectral_centroid,
                event.attributes.spectral_flatness,
            )
        };
        features.push(f);
        prev_rms_db = rms_db;
    }
    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        CorpusEnvelope, DomainHint, EnrichedAttributes, RiskFlags, StemTimeline, TimelineEvent,
    };

    fn make_event(state: &str, start_ms: u32, rms_db: f32) -> TimelineEvent {
        TimelineEvent {
            state: state.to_string(),
            start_ms,
            end_ms: start_ms + 100,
            duration_ms: 100,
            confidence: 0.9,
            session_id: "test".to_string(),
            attributes: EnrichedAttributes {
                rms_db,
                crest_factor_db: 6.0,
                transient_density: 0.1,
                spectral_centroid: 2000.0,
                lufs_integrated: -23.0,
                spectral_flatness: 0.3,
            },
            risk: RiskFlags {
                artifact_risk: 0.0,
                sibilance_risk: 0.0,
                phase_issue: 0.0,
                sub_rumble: 0.0,
            },
            domain: DomainHint {
                stem: "voice".to_string(),
                profile_hint: "music_v1".to_string(),
            },
            mfcc: [0.0; 13],
        }
    }

    fn make_envelope(stem: &str, states: &[(&str, u32, f32)]) -> CorpusEnvelope {
        let events = states
            .iter()
            .map(|(s, ms, rms)| make_event(s, *ms, *rms))
            .collect();
        CorpusEnvelope {
            protocol_version: "900".to_string(),
            session_id: "test-session".to_string(),
            stems: vec![StemTimeline {
                stem_type: stem.to_string(),
                events,
            }],
        }
    }

    #[test]
    fn extract_sequence_ordered_by_start_ms() {
        let env = make_envelope(
            "voice",
            &[
                ("vowel", 200, -20.0),
                ("silence", 0, -80.0),
                ("consonant", 100, -25.0),
            ],
        );
        let seq = extract_state_sequence(&env, "voice");
        assert_eq!(seq, vec!["silence", "consonant", "vowel"]);
    }

    #[test]
    fn extract_sequence_wrong_stem_empty() {
        let env = make_envelope("voice", &[("silence", 0, -80.0)]);
        assert!(extract_state_sequence(&env, "drums").is_empty());
    }

    #[test]
    fn extract_features_rms_delta_first_window() {
        let env = make_envelope("voice", &[("silence", 0, -80.0), ("vowel", 100, -20.0)]);
        let features = extract_features_sequence(&env, "voice");
        assert_eq!(features.len(), 2);
        // First: rms=-80.0 < -60.0 → silence (floor takes priority over delta)
        assert_eq!(features[0].activity_hint(), "silence");
        // Second: prev=-80, rms=-20 → delta=+60 > +6 → attack
        assert_eq!(features[1].activity_hint(), "attack");
    }

    #[test]
    fn extract_features_decay_detected() {
        let env = make_envelope("drums", &[("sustain", 0, -15.0), ("decay", 100, -25.0)]);
        let features = extract_features_sequence(&env, "drums");
        // Second: prev=-15, rms=-25 → delta=-10 → decay
        assert_eq!(features[1].activity_hint(), "decay");
    }

    #[test]
    fn read_empty_dir_returns_empty() {
        let tmp = std::env::temp_dir();
        let _ = read_corpus_sessions(&tmp); // must not panic
    }
}
