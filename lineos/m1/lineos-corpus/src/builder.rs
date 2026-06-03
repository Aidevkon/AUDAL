use lineos_types::analysis::{StemFeatures, StemMetrics};
use lineos_types::pre_analysis::PreAnalysisData;
use crate::contract::*;
use aether::markov::voice_v1::MarkovStateClassifier;
use aether::markov::drums_v1::DrumsMarkovStateClassifier;
use aether::markov::bass_v1::BassMarkovStateClassifier;
use aether::markov::harmonics_v1::HarmonicsMarkovStateClassifier;
use aether::markov::ambience_v1::AmbienceMarkovStateClassifier;

/// Build 900-JSON CorpusEnvelope from a single mastering run.
/// v1.0: one aggregate event per stem (frame-by-frame in v2.0)
/// INV-CP-1: deterministic — same input → same envelope
pub fn build_timeline(
    features:     &StemFeatures,
    pre_analysis: &PreAnalysisData,
    blob_id:      &str,
    duration_ms:  u32,
    flavour_id:   &str,
) -> CorpusEnvelope {
    let session_id = blob_id.to_string();

    let stems = vec![
        build_stem_timeline("voice",     &features.voice,
            MarkovStateClassifier::classify_voice(&features.voice).to_str(),
            &session_id, duration_ms, pre_analysis, flavour_id),
        build_stem_timeline("drums",     &features.drums,
            DrumsMarkovStateClassifier::classify_drums(&features.drums).to_str(),
            &session_id, duration_ms, pre_analysis, flavour_id),
        build_stem_timeline("bass",      &features.bass,
            BassMarkovStateClassifier::classify_bass(&features.bass).to_str(),
            &session_id, duration_ms, pre_analysis, flavour_id),
        build_stem_timeline("harmonics", &features.harmonics,
            HarmonicsMarkovStateClassifier::classify_harmonics(&features.harmonics).to_str(),
            &session_id, duration_ms, pre_analysis, flavour_id),
        build_stem_timeline("ambience",  &features.ambience,
            AmbienceMarkovStateClassifier::classify_ambience(&features.ambience).to_str(),
            &session_id, duration_ms, pre_analysis, flavour_id),
    ];

    CorpusEnvelope {
        protocol_version: PROTOCOL_VERSION.to_string(),
        session_id,
        stems,
    }
}

const FRAME_SIZE_MS: u32 = 100;

fn build_stem_timeline(
    stem_type: &str,
    metrics: &StemMetrics,
    state_str: &str,
    session_id: &str,
    duration_ms: u32,
    pre_analysis: &PreAnalysisData,
    flavour_id: &str,
) -> StemTimeline {
    let mut events = Vec::new();
    let n_frames = (duration_ms / FRAME_SIZE_MS).max(1);
    
    for frame_idx in 0..n_frames {
        let start_ms = frame_idx * FRAME_SIZE_MS;
        let end_ms = (start_ms + FRAME_SIZE_MS).min(duration_ms);
        events.push(build_frame_event(
            stem_type,
            metrics,
            state_str,
            session_id,
            start_ms,
            end_ms,
            pre_analysis,
            flavour_id,
        ));
    }
    
    StemTimeline {
        stem_type: stem_type.to_string(),
        events,
    }
}

fn build_frame_event(
    stem_type: &str,
    metrics: &StemMetrics,
    state_str: &str,
    session_id: &str,
    start_ms: u32,
    end_ms: u32,
    pre_analysis: &PreAnalysisData,
    flavour_id: &str,
) -> TimelineEvent {
    TimelineEvent {
        state: state_str.to_string(),
        start_ms,
        end_ms,
        duration_ms: end_ms.saturating_sub(start_ms),
        confidence: 1.0,
        session_id: session_id.to_string(),
        attributes: EnrichedAttributes {
            rms_db: metrics.rms_db,
            crest_factor_db: metrics.crest_factor_db,
            transient_density: metrics.transient_density,
            spectral_centroid: metrics.spectral_centroid_hz,
            lufs_integrated: pre_analysis.integrated_lufs,
            spectral_flatness: 0.0,
        },
        risk: RiskFlags {
            artifact_risk: 0.0,
            sibilance_risk: 0.0,
            phase_issue: if pre_analysis.zone_flags.zone_phase_issue { 1.0 } else { 0.0 },
            sub_rumble: if pre_analysis.zone_flags.zone_sub_rumble { 1.0 } else { 0.0 },
        },
        domain: DomainHint {
            stem: stem_type.to_string(),
            profile_hint: flavour_id.to_string(),
        }
    }
}
