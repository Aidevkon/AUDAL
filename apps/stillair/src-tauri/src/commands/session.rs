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

use crate::aether::CoachNarrativeJson;
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
pub async fn get_session_state(blob_id: String) -> Result<SessionStateJson, String> {
    eprintln!("[get_session_state] START blob_id={blob_id}");

    // Step 1: Fetch GoldenBlobJson from M0
    let client = M0Client::new();
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

    eprintln!("[get_session_state] DONE — returning SessionStateJson");
    Ok(SessionStateJson {
        blob_id:  blob_id,
        loudness: blob.loudness,
        quality:  blob.quality,
        compliance,
        findings,
        narrative,
    })
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
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"blob_id\":\"test-blob-001\""));
        assert!(json.contains("\"narrative\":null"));
        assert!(json.contains("\"compliance\""));
    }
}
