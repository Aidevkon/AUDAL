//! Per-user model store — 3-level hierarchy.
//! Authority: corpus-learning-spec-v1_2.md CB-P6
//!
//! Hierarchy: User → Preset → Stem
//! INV-CB-8: Preset models NEVER cross-contaminate.
//! INV-CB-2: Incremental updates — never full retrain.

use std::collections::HashMap;
use crate::inference::StemMarkovModel;
use crate::reader::{extract_state_sequence, extract_features_sequence};
use crate::contract::CorpusEnvelope;

const STEM_TYPES: &[&str] = &["voice", "drums", "bass", "harmonics", "ambience"];

/// Model for one genre/preset — isolated learning domain.
/// Techno drums NEVER contaminate podcast voice.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PresetMarkovModel {
    pub preset_id: String,
    pub stems:     HashMap<String, StemMarkovModel>,
}

impl PresetMarkovModel {
    pub fn new(preset_id: &str) -> Self {
        Self {
            preset_id: preset_id.to_string(),
            stems:     HashMap::new(),
        }
    }

    /// Train on a corpus session for all stems.
    pub fn train(&mut self, session: &CorpusEnvelope) {
        for stem_type in STEM_TYPES {
            let states   = extract_state_sequence(session, stem_type);
            let features = extract_features_sequence(session, stem_type);
            if states.is_empty() { continue; }
            self.stems
                .entry(stem_type.to_string())
                .or_insert_with(|| StemMarkovModel::new(stem_type))
                .train(&states, &features);
        }
    }

    /// Get model for a specific stem.
    pub fn stem(&self, stem_type: &str) -> Option<&StemMarkovModel> {
        self.stems.get(stem_type)
    }
}

/// Per-user model store.
/// One PresetMarkovModel per preset_id — isolated per genre.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserMarkovModel {
    pub user_id:    String,
    pub version:    u32,
    pub presets:    HashMap<String, PresetMarkovModel>,
    pub updated_at: u64,
}

impl UserMarkovModel {
    pub fn new(user_id: &str) -> Self {
        Self {
            user_id:    user_id.to_string(),
            version:    1,
            presets:    HashMap::new(),
            updated_at: 0,
        }
    }

    /// Update model for a specific preset_id with new session data.
    /// Creates PresetMarkovModel if preset not seen before.
    /// INV-CB-2: incremental — merges counts, never full retrain.
    /// INV-CB-8: only updates the given preset — no cross-contamination.
    pub fn update(&mut self, preset_id: &str, session: &CorpusEnvelope) {
        self.presets
            .entry(preset_id.to_string())
            .or_insert_with(|| PresetMarkovModel::new(preset_id))
            .train(session);
        self.version += 1;
    }

    /// Get model for a specific preset.
    pub fn preset(&self, preset_id: &str) -> Option<&PresetMarkovModel> {
        self.presets.get(preset_id)
    }

    /// Total sessions trained across all presets.
    pub fn total_sessions(&self) -> usize {
        self.presets.values()
            .flat_map(|p| p.stems.values())
            .map(|s| s.n_sessions)
            .max()
            .unwrap_or(0)
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        CorpusEnvelope, StemTimeline, TimelineEvent,
        EnrichedAttributes, RiskFlags, DomainHint,
    };

    fn make_session(preset_hint: &str) -> CorpusEnvelope {
        let events = vec![
            TimelineEvent {
                state: "silence".to_string(),
                start_ms: 0, end_ms: 100, duration_ms: 100,
                confidence: 0.9,
                session_id: "test".to_string(),
                attributes: EnrichedAttributes {
                    rms_db: -80.0, crest_factor_db: 6.0,
                    transient_density: 0.0, spectral_centroid: 1000.0,
                    lufs_integrated: -70.0, spectral_flatness: 0.5,
                },
                risk: RiskFlags { artifact_risk: 0.0, sibilance_risk: 0.0,
                    phase_issue: 0.0, sub_rumble: 0.0 },
                domain: DomainHint {
                    stem: "voice".to_string(),
                    profile_hint: preset_hint.to_string(),
                },
            },
            TimelineEvent {
                state: "vowel".to_string(),
                start_ms: 100, end_ms: 200, duration_ms: 100,
                confidence: 0.9,
                session_id: "test".to_string(),
                attributes: EnrichedAttributes {
                    rms_db: -20.0, crest_factor_db: 6.0,
                    transient_density: 0.1, spectral_centroid: 2000.0,
                    lufs_integrated: -14.0, spectral_flatness: 0.2,
                },
                risk: RiskFlags { artifact_risk: 0.0, sibilance_risk: 0.0,
                    phase_issue: 0.0, sub_rumble: 0.0 },
                domain: DomainHint {
                    stem: "voice".to_string(),
                    profile_hint: preset_hint.to_string(),
                },
            },
        ];
        CorpusEnvelope {
            protocol_version: "900".to_string(),
            session_id: "test-session".to_string(),
            stems: vec![StemTimeline {
                stem_type: "voice".to_string(),
                events,
            }],
        }
    }

    #[test]
    fn update_creates_preset() {
        let mut model = UserMarkovModel::new("anestis");
        let session = make_session("techno");
        model.update("techno", &session);
        assert!(model.preset("techno").is_some());
        assert!(model.preset("podcast").is_none());
    }

    #[test]
    fn presets_isolated() {
        let mut model = UserMarkovModel::new("anestis");
        model.update("techno",  &make_session("techno"));
        model.update("podcast", &make_session("podcast"));
        assert!(model.preset("techno").is_some());
        assert!(model.preset("podcast").is_some());
        // Techno stem model must not be same as podcast
        let techno_sessions = model.preset("techno")
            .and_then(|p| p.stem("voice"))
            .map(|s| s.n_sessions).unwrap_or(0);
        let podcast_sessions = model.preset("podcast")
            .and_then(|p| p.stem("voice"))
            .map(|s| s.n_sessions).unwrap_or(0);
        assert_eq!(techno_sessions, 1);
        assert_eq!(podcast_sessions, 1);
    }

    #[test]
    fn version_increments_on_update() {
        let mut model = UserMarkovModel::new("anestis");
        assert_eq!(model.version, 1);
        model.update("techno", &make_session("techno"));
        assert_eq!(model.version, 2);
        model.update("techno", &make_session("techno"));
        assert_eq!(model.version, 3);
    }

    #[test]
    fn json_roundtrip() {
        let mut model = UserMarkovModel::new("anestis");
        model.update("techno", &make_session("techno"));
        let json = model.to_json().expect("serialize failed");
        let restored = UserMarkovModel::from_json(&json)
            .expect("deserialize failed");
        assert_eq!(restored.user_id, "anestis");
        assert!(restored.preset("techno").is_some());
    }

    #[test]
    fn incremental_train_merges() {
        let mut model = UserMarkovModel::new("anestis");
        model.update("techno", &make_session("techno"));
        model.update("techno", &make_session("techno"));
        let sessions = model.preset("techno")
            .and_then(|p| p.stem("voice"))
            .map(|s| s.n_sessions).unwrap_or(0);
        assert_eq!(sessions, 2);
    }
}
