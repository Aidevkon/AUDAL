//! P9-008 — Session State Unification.
//! Authority: Phase 9, Still Air state-machine.md §4
//!
//! Single Tauri command get_session_state(blob_id) that returns a complete
//! SessionStateJson: loudness + quality + compliance + findings + narrative.
//!
//! This is the canonical entry point for the Dioxus Cockpit (Phase 11).
//! It replaces the three separate Data Cascade calls in the Leptos frontend
//! (get_golden_blob → evaluate_findings → get_coach_narrative) with one
//! atomic snapshot.
//!
//! Architecture:
//!   get_session_state(blob_id)
//!     1. GET /blob/{blob_id} → M0 → GoldenBlobJson
//!     2. evaluate_findings(blob) → rule-engine (in-process) → CoachFindingsJson
//!     3. get_coach_narrative(findings) → CoachAdapter → CoachNarrativeJson (best-effort)
//!     → SessionStateJson composed from all three
//!
//! FORBIDDEN (Amendment A-002 §3):
//!   ❌ Audio processing logic in this command
//!   ❌ Loudness re-measurement
//!   ❌ Rule evaluation outside lineos-rule-engine
//!   ❌ LLM invocation outside CoachAdapter / adapter-runtime

use serde::{Deserialize, Serialize};
use tokio::time::{timeout, Duration};

use crate::coach_narrative::CoachNarrativeJson;
use crate::commands::insights::{evaluate_findings, CoachFindingsJson};
use crate::commands::coach::get_coach_narrative;
use crate::ipc::m0_client::{GoldenBlobJson, LoudnessMetricsJson, M0Client, QualityMetricsJson};

/// Maximum time to wait for Ollama coach inference.
/// If exceeded, narrative = None (non-fatal). Cockpit still transitions to FM5.
const COACH_TIMEOUT_SECS: u64 = 25;

// ── SessionStateJson ──────────────────────────────────────────────────────────

/// Compliance summary derived from GoldenBlobJson loudness flags.
/// All fields are pre-computed by the mastering pipeline — never re-derived here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceJson {
    pub spotify:   bool,
    pub youtube:   bool,
    pub apple:     bool,
    pub tidal:     bool,
    pub broadcast: bool,
    pub ebu_r128:  bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ZoneFlagsJson {
    pub zone_cymbal_harsh:    bool,
    pub zone_sub_rumble:      bool,
    pub zone_boxiness:        bool,
    pub zone_phase_issue:     bool,
    pub zone_harsh_resonance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationResultJson {
    pub passed:         bool,
    pub trim_applied_db: f32,
    pub was_trimmed:    bool,
    pub warning:        Option<String>,
}

/// JINI suggestion — personality-aware mastering recommendation.
/// Authority: JINI Spec v1.0 J-P8
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JiniSuggestionJson {
    pub narrative:    String,
    pub action_type:  String,     // "macro_change" | "flavour_switch" | "nothing"
    pub action_label: String,     // human readable e.g. "Switch to Clean mode"
    pub confidence:   f32,
}

/// Complete session snapshot for a mastered Golden Blob.
///
/// Phase 11 (Dioxus Cockpit): this is the single IPC call that replaces
/// the three-step Data Cascade. The Dioxus UI calls get_session_state(blob_id)
/// once and receives a fully composed view-model.
///
/// Fields mirror the Leptos frontend signals (mode, metrics, findings, narrative)
/// in a single serialized structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStateJson {
    /// Golden Blob ID this snapshot was generated from.
    pub blob_id:    String,

    /// BS.1770-4 canonical loudness measurements.
    /// Source: GoldenBlobJson.loudness — never re-measured.
    pub loudness:   LoudnessMetricsJson,

    /// Objective quality measurements (stereo, dynamic range, clipping).
    /// Source: GoldenBlobJson.quality — never re-measured.
    pub quality:    QualityMetricsJson,

    /// Platform compliance summary derived from loudness flags.
    /// Source: GoldenBlobJson.loudness.*_compliant fields.
    pub compliance: ComplianceJson,

    /// Rule-engine findings — deterministic, computed in-process.
    /// Source: lineos-rule-engine::evaluate() on GoldenBlobJson metrics.
    pub findings:   CoachFindingsJson,

    /// Coach narrative — stochastic LLM output, None if unavailable.
    /// Unavailability is non-fatal: Ollama may not be running, provider
    /// may be misconfigured, or narrative generation may have timed out.
    pub narrative:  Option<CoachNarrativeJson>,

    #[serde(default)]
    pub aether_cert:    Option<String>,
    #[serde(default)]
    pub aether_persona: Option<String>,
    #[serde(default)]
    pub aether_config:  Option<String>,
    
    #[serde(default)]
    pub zone_flags: Option<ZoneFlagsJson>,

    #[serde(default)]
    pub verification: Option<VerificationResultJson>,

    /// JINI suggestion — personality-aware mastering recommendation.
    /// Authority: JINI Spec v1.0 J-P8
    #[serde(default)]
    pub jini: Option<JiniSuggestionJson>,

    #[serde(default)]
    pub dsp_chain: Option<DspChainStateJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DspChainStateJson {
    pub eq_active:   bool,
    pub comp_active: bool,
    pub sat_active:  bool,
    pub limit_active: bool,
}

// ── Tauri command ─────────────────────────────────────────────────────────────

/// P9-008: Get complete session state for a mastered Golden Blob.
///
/// Single entry point for the Dioxus Cockpit (Phase 11). Atomically composes:
///   loudness + quality + compliance (from M0) +
///   findings (rule-engine, in-process) +
///   narrative (CoachAdapter, best-effort)
///
/// Errors:
///   - blob not found → Err (blob_id invalid or M0 restarted)
///   - rule-engine failure → Err (invariant violation, should not happen)
///   - narrative failure → narrative = None (non-fatal, not surfaced as error)
#[tauri::command]
pub async fn get_session_state(
    blob_id: String,
    persona: Option<String>,
    client:  tauri::State<'_, M0Client>,
) -> Result<SessionStateJson, String> {
    eprintln!("[get_session_state] START blob_id={blob_id}");

    // Step 1: Fetch GoldenBlobJson from M0
    let blob: GoldenBlobJson = client
        .get_blob(&blob_id)
        .await
        .map_err(|e| format!("Session: blob fetch failed: {e}"))?;
    eprintln!("[get_session_state] blob fetched ok (lufs={})", blob.loudness.integrated_lufs);

    // Step 2: Evaluate findings via rule-engine (in-process, deterministic)
    let findings: CoachFindingsJson = evaluate_findings(blob.clone())
        .await
        .map_err(|e| format!("Session: rule-engine failed: {e}"))?;
    eprintln!("[get_session_state] findings ok ({} issues)", findings.issues.len());

    // Step 3: Get coach narrative (best-effort, 25s timeout)
    eprintln!("[get_session_state] calling coach (max {}s)...", COACH_TIMEOUT_SECS);
    let narrative: Option<CoachNarrativeJson> =
        match timeout(
            Duration::from_secs(COACH_TIMEOUT_SECS),
            get_coach_narrative(findings.clone()),
        ).await {
            Ok(Ok(n))  => {
                eprintln!("[get_session_state] coach narrative ok");
                Some(n)
            }
            Ok(Err(e)) => {
                eprintln!("[get_session_state] coach narrative err (non-fatal): {e}");
                None
            }
            Err(_elapsed) => {
                eprintln!("[get_session_state] coach timeout after {COACH_TIMEOUT_SECS}s — returning None");
                None
            }
        };

    // Step 4: Compose compliance summary from pre-computed loudness flags
    let compliance = ComplianceJson {
        spotify:   blob.loudness.spotify_compliant,
        youtube:   blob.loudness.youtube_compliant,
        apple:     blob.loudness.apple_music_compliant,
        tidal:     blob.loudness.tidal_compliant,
        broadcast: blob.loudness.broadcast_compliant,
        ebu_r128:  blob.loudness.ebu_r128_compliant,
    };

    use lineos_types::JiniPersonaId;
    let persona_id = match persona.as_deref() {
        Some("beginner") => JiniPersonaId::Beginner,
        Some("pro")      => JiniPersonaId::Pro,
        _                => JiniPersonaId::Intermediate,
    };

    // Step 5 (J-P8): Build JINI suggestion from quality + zone flags
    let jini = build_jini_suggestion(
        &blob.quality,
        &ZoneFlagsJson::default(),
        blob.loudness.integrated_lufs,
        persona_id,
    );

    let dsp_chain = blob.aether_config.as_ref().and_then(|cfg_str| {
        serde_json::from_str::<serde_json::Value>(cfg_str).ok().map(|cfg| {
            DspChainStateJson {
                eq_active:    cfg["eq"]["low_shelf_gain_db"].as_f64().unwrap_or(0.0).abs() > 0.01
                           || cfg["eq"]["high_shelf_gain_db"].as_f64().unwrap_or(0.0).abs() > 0.01
                           || cfg["eq"]["zone_bands"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
                comp_active:  cfg["dynamics"]["comp_ratio"].as_f64().unwrap_or(1.0) > 1.0,
                sat_active:   cfg["sat"]["drive"].as_f64().unwrap_or(0.0) > 0.0
                           && cfg["sat"]["mix"].as_f64().unwrap_or(0.0) > 0.0,
                limit_active: true,
            }
        })
    });

    eprintln!("[get_session_state] DONE — returning SessionStateJson");
    Ok(SessionStateJson {
        blob_id,
        loudness: blob.loudness,
        quality:  blob.quality,
        compliance,
        findings,
        narrative,
        aether_cert:    blob.aether_cert.clone(),
        aether_persona: blob.aether_persona.clone(),
        aether_config:  blob.aether_config.clone(),
        zone_flags:     Some(ZoneFlagsJson::default()),
        verification:   Some(VerificationResultJson {
            passed: true,
            trim_applied_db: 0.0,
            was_trimmed: false,
            warning: None,
        }),
        jini: Some(jini),
        dsp_chain,
    })
}

// ── JINI suggestion builder (J-P8) ───────────────────────────────────────────

/// Build a JINI suggestion from quality metrics + zone flags.
/// Uses lineos-types BehaviourVector mapping — inlined here because
/// sp314-dsp cannot be imported into Tauri (Amendment A-002 §3).
/// Rule-based, deterministic. No LLM, no network.
fn build_jini_suggestion(
    quality: &QualityMetricsJson,
    zones:   &ZoneFlagsJson,
    lufs:    f32,
    persona_id: lineos_types::JiniPersonaId,
) -> JiniSuggestionJson {
    use lineos_types::*;

    let behaviour = BehaviourVector {
        loudness: if lufs < -18.0      { LoudnessBehaviour::TooQuiet }
                  else if lufs > -8.0  { LoudnessBehaviour::TooLoud }
                  else                 { LoudnessBehaviour::Balanced },
        spectral: if zones.zone_boxiness       { SpectralBehaviour::Boxy }
                  else if zones.zone_cymbal_harsh { SpectralBehaviour::Harsh }
                  else                           { SpectralBehaviour::Neutral },
        dynamics: if quality.dynamic_range_db < 6.0 { DynamicsBehaviour::Overcompressed }
                  else                              { DynamicsBehaviour::Stable },
        stereo:   if quality.stereo_correlation < 0.3 { StereoBehaviour::Unstable }
                  else if quality.stereo_width < 0.1  { StereoBehaviour::Mono }
                  else                                { StereoBehaviour::Wide },
        quality:  if quality.clips_detected > 0 { QualityBehaviour::Clipping }
                  else                          { QualityBehaviour::Clean },
    };

    // Priority: Quality → Loudness → Spectral → Dynamics → Stereo
    // Mirrors sp314-dsp/src/jini/mod.rs rule_based_suggestion() logic
    let _persona = persona_id;

    let (narrative, action, confidence) = if behaviour.quality == QualityBehaviour::Clipping {
        (
            "Clipping detected — consider reducing input gain before mastering.".to_string(),
            Some(JiniAction::SuggestMacroChange {
                handle: MacroHandle::Loudness,
                delta: -0.2,
                reason: "Clipping detected".to_string(),
            }),
            0.95,
        )
    } else if behaviour.loudness == LoudnessBehaviour::TooLoud {
        (
            "The mix is quite hot — you might want to bring the loudness down for better dynamics.".to_string(),
            Some(JiniAction::SuggestMacroChange {
                handle: MacroHandle::Loudness,
                delta: -0.15,
                reason: "Loudness exceeds target range".to_string(),
            }),
            0.85,
        )
    } else if behaviour.loudness == LoudnessBehaviour::TooQuiet {
        (
            "The track is very quiet — a small loudness boost would help it compete.".to_string(),
            Some(JiniAction::SuggestMacroChange {
                handle: MacroHandle::Loudness,
                delta: 0.15,
                reason: "Loudness below target range".to_string(),
            }),
            0.80,
        )
    } else if behaviour.spectral != SpectralBehaviour::Neutral {
        let (desc, handle, delta) = match behaviour.spectral {
            SpectralBehaviour::Boxy  => ("Some boxiness in the low-mids", MacroHandle::Tone, -0.1),
            SpectralBehaviour::Harsh => ("Harshness in the upper frequencies", MacroHandle::Tone, -0.1),
            _ => ("Spectral balance could be improved", MacroHandle::Tone, 0.0),
        };
        (
            format!("{desc} — a tone adjustment could help."),
            Some(JiniAction::SuggestMacroChange {
                handle,
                delta,
                reason: desc.to_string(),
            }),
            0.75,
        )
    } else if behaviour.dynamics == DynamicsBehaviour::Overcompressed {
        (
            "The mix sounds a bit squashed — easing the dynamics could restore some life.".to_string(),
            Some(JiniAction::SuggestMacroChange {
                handle: MacroHandle::Dynamics,
                delta: -0.1,
                reason: "Over-compressed dynamic range".to_string(),
            }),
            0.70,
        )
    } else if behaviour.stereo == StereoBehaviour::Unstable {
        (
            "Stereo correlation is low — check for phase issues.".to_string(),
            Some(JiniAction::SuggestMacroChange {
                handle: MacroHandle::Width,
                delta: -0.1,
                reason: "Low stereo correlation".to_string(),
            }),
            0.65,
        )
    } else {
        (
            "Everything looks good — the mix is well-balanced.".to_string(),
            Some(JiniAction::SuggestNothing),
            0.90,
        )
    };

    let action_ref = action.as_ref().unwrap_or(&JiniAction::SuggestNothing);
    let action_label = match action_ref {
        JiniAction::SuggestMacroChange { handle, delta, .. } =>
            format!("{} {:?} by {:.0}%",
                if *delta < 0.0 { "Reduce" } else { "Increase" },
                handle, delta.abs() * 100.0),
        JiniAction::SuggestFlavourSwitch { to, .. } =>
            format!("Switch to {:?} mode", to),
        JiniAction::SuggestNothing => String::new(),
    };

    let action_type = match action_ref {
        JiniAction::SuggestMacroChange { .. }   => "macro_change",
        JiniAction::SuggestFlavourSwitch { .. } => "flavour_switch",
        JiniAction::SuggestNothing              => "nothing",
    };

    JiniSuggestionJson {
        narrative,
        action_type:  action_type.to_string(),
        action_label,
        confidence,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    fn make_loudness(lufs: f32, tp: f32) -> LoudnessMetricsJson {
        LoudnessMetricsJson {
            integrated_lufs:          lufs,
            short_term_lufs:          lufs + 0.5,
            momentary_lufs:           lufs + 1.0,
            true_peak_dbtp:           tp,
            lra:                      6.0,
            k_weighted:               true,
            ebu_r128_target_lufs:     -23.0,
            ebu_r128_compliant:       lufs <= -23.0 && tp <= -1.0,
            spotify_compliant:        lufs <= -13.0 && tp <= -1.0,
            youtube_compliant:        lufs <= -13.0 && tp <= -1.0,
            apple_music_compliant:    lufs <= -15.0 && tp <= -1.0,
            apple_podcasts_compliant: lufs <= -15.0 && tp <= -1.0,
            broadcast_compliant:      lufs <= -22.0 && tp <= -1.0,
            tidal_compliant:          lufs <= -13.0 && tp <= -1.0,
        }
    }

    fn make_quality() -> QualityMetricsJson {
        QualityMetricsJson {
            stereo_correlation: 0.94,
            phase_coherence:    0.97,
            stereo_width:       0.74,
            dynamic_range_db:   9.5,
            rms_db:             -16.0,
            spectral_centroid:  3_200.0,
            spectral_flatness:  0.12,
            clips_detected:     0,
            clip_free:          true,
        }
    }

    #[test]
    fn test_compliance_derived_from_loudness_flags() {
        let loudness = make_loudness(-14.0, -1.5);
        let compliance = ComplianceJson {
            spotify:   loudness.spotify_compliant,
            youtube:   loudness.youtube_compliant,
            apple:     loudness.apple_music_compliant,
            tidal:     loudness.tidal_compliant,
            broadcast: loudness.broadcast_compliant,
            ebu_r128:  loudness.ebu_r128_compliant,
        };
        assert!(compliance.spotify);
        assert!(compliance.youtube);
        assert!(compliance.tidal);
        assert!(!compliance.apple);     // -14 > -15 target
        assert!(!compliance.broadcast); // -14 > -22 target
        assert!(!compliance.ebu_r128);  // -14 > -23 target
    }

    #[test]
    fn test_session_state_json_serializes() {
        let state = SessionStateJson {
            blob_id:    "test-blob-001".into(),
            loudness:   make_loudness(-14.0, -1.5),
            quality:    make_quality(),
            compliance: ComplianceJson {
                spotify: true, youtube: true, apple: false,
                tidal: true, broadcast: false, ebu_r128: false,
            },
            findings: CoachFindingsJson {
                issues:         vec![],
                recommendation: "Master sounds great. No critical issues.".into(),
            },
            narrative: None,
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            zone_flags: Some(ZoneFlagsJson::default()),
            verification: Some(VerificationResultJson {
                passed: true,
                trim_applied_db: 0.0,
                was_trimmed: false,
                warning: None,
            }),
            jini: None,
            dsp_chain: None,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"blob_id\":\"test-blob-001\""));
        assert!(json.contains("\"narrative\":null"));
        assert!(json.contains("\"compliance\""));
    }
}
