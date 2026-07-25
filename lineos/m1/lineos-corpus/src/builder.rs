use crate::contract::*;
use lineos_types::analysis::{StemFeatures, StemMetrics};
use lineos_types::pre_analysis::PreAnalysisData;

/// Classify state from window metrics — per stem type
pub(crate) fn classify_for_stem(stem_type: &str, rms_db: f32, td: f32) -> &'static str {
    match stem_type {
        "voice" => {
            if rms_db < -50.0 {
                "silence"
            } else if rms_db < -35.0 && td < 0.08 {
                "breath"
            } else if td > 0.15 {
                "consonant"
            } else if rms_db > -30.0 {
                "vowel"
            } else {
                "tail"
            }
        }
        "drums" => {
            if rms_db < -50.0 {
                "quiet"
            } else if td > 0.30 {
                "transient"
            } else if rms_db > -20.0 {
                "sustain"
            } else {
                "decay"
            }
        }
        "bass" => {
            if rms_db < -50.0 {
                "silent"
            } else if td > 0.20 {
                "punchy"
            } else if rms_db > -25.0 {
                "sustained"
            } else {
                "rumble"
            }
        }
        "harmonics" => {
            if rms_db < -50.0 {
                "silent"
            } else if rms_db > -20.0 {
                "dense"
            } else {
                "sparse"
            }
        }
        _ => {
            // ambience
            if rms_db < -50.0 {
                "dry"
            } else if rms_db > -25.0 {
                "lush"
            } else {
                "subtle"
            }
        }
    }
}

// allow: 8 args; a params-struct refactor is deliberately deferred — not done as a clippy side-fix
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_windowed_stem(
    signal: &[f32],
    stem_type: &str,
    session_id: &str,
    base_td: f32,
    base_attrs: &EnrichedAttributes,
    base_risk: &RiskFlags,
    sample_rate: u32,
    mfcc_analyzer: &mut crate::mfcc::MfccAnalyzer,
) -> Vec<TimelineEvent> {
    let window_samples = (sample_rate as f32 * 0.1) as usize; // 100ms
    let window_samples = window_samples.max(1);
    let n_windows = (signal.len() / window_samples).max(1);
    let mut events = Vec::with_capacity(n_windows);

    for w in 0..n_windows {
        let start = w * window_samples;
        let end = ((w + 1) * window_samples).min(signal.len());
        let window = &signal[start..end];

        let sum_sq: f32 = window.iter().map(|s| s * s).sum();
        let rms_db = if sum_sq > 1e-30 {
            10.0 * (sum_sq / window.len() as f32).log10()
        } else {
            -144.0
        };

        let state = classify_for_stem(stem_type, rms_db, base_td);

        let window_audio = &signal[start..end];
        let window_mfcc = mfcc_analyzer.compute(window_audio);

        events.push(TimelineEvent {
            state: state.to_string(),
            start_ms: (w as u32) * 100,
            end_ms: (w as u32 + 1) * 100,
            duration_ms: 100,
            session_id: session_id.to_string(),
            confidence: 1.0,
            attributes: EnrichedAttributes {
                rms_db,
                transient_density: base_td,
                ..base_attrs.clone()
            },
            risk: base_risk.clone(),
            domain: DomainHint {
                stem: stem_type.to_string(),
                profile_hint: "auto".to_string(),
            },
            mfcc: window_mfcc,
        });
    }
    events
}

pub(crate) fn features_for<'a>(stem: &str, f: &'a StemFeatures) -> &'a StemMetrics {
    match stem {
        "voice" => &f.voice,
        "drums" => &f.drums,
        "bass" => &f.bass,
        "harmonics" => &f.harmonics,
        _ => &f.ambience,
    }
}

// allow: 10 args; a params-struct refactor is deliberately deferred — not done as a clippy side-fix
#[allow(clippy::too_many_arguments)]
pub fn build_timeline(
    features: &StemFeatures,
    voice_audio: &[f32],
    drums_audio: &[f32],
    bass_audio: &[f32],
    harmonics_audio: &[f32],
    ambience_audio: &[f32],
    pre_analysis: &PreAnalysisData,
    blob_id: &str,
    sample_rate: u32,
    _flavour_id: &str,
) -> CorpusEnvelope {
    let session_id = blob_id;

    let stem_pairs: &[(&str, &[f32], f32)] = &[
        ("voice", voice_audio, features.voice.transient_density),
        ("drums", drums_audio, features.drums.transient_density),
        ("bass", bass_audio, features.bass.transient_density),
        (
            "harmonics",
            harmonics_audio,
            features.harmonics.transient_density,
        ),
        (
            "ambience",
            ambience_audio,
            features.ambience.transient_density,
        ),
    ];

    let mut mfcc_analyzer = crate::mfcc::MfccAnalyzer::new();

    let stem_timelines: Vec<StemTimeline> = stem_pairs
        .iter()
        .map(|(name, signal, td)| {
            let base_attrs = EnrichedAttributes {
                rms_db: features_for(name, features).rms_db,
                crest_factor_db: features_for(name, features).crest_factor_db,
                transient_density: *td,
                spectral_centroid: features_for(name, features).spectral_centroid_hz,
                lufs_integrated: pre_analysis.integrated_lufs,
                spectral_flatness: features_for(name, features).spectral_flatness,
            };
            let base_risk = RiskFlags {
                artifact_risk: 0.0,
                sibilance_risk: if pre_analysis.zone_flags.zone_cymbal_harsh {
                    0.7
                } else {
                    0.0
                },
                phase_issue: if pre_analysis.zone_flags.zone_phase_issue {
                    0.8
                } else {
                    0.0
                },
                sub_rumble: if pre_analysis.zone_flags.zone_sub_rumble {
                    1.0
                } else {
                    0.0
                },
            };
            StemTimeline {
                stem_type: name.to_string(),
                events: build_windowed_stem(
                    signal,
                    name,
                    session_id,
                    *td,
                    &base_attrs,
                    &base_risk,
                    sample_rate,
                    &mut mfcc_analyzer,
                ),
            }
        })
        .collect();

    CorpusEnvelope {
        protocol_version: "900".to_string(),
        session_id: blob_id.to_string(),
        stems: stem_timelines,
    }
}
