//! Insights command — rule-engine evaluation from Golden Blob.
//! Authority: Phase 6 task-decomposition P6-006
//!
//! Receives GoldenBlobJson from frontend (already in memory from get_golden_blob).
//! Runs lineos-rule-engine locally — no network call needed (pure deterministic).
//! Returns CoachFindingsJson — typed primitives, no serde_json::Value.
//!
//! FORBIDDEN: Re-measuring audio (all values come from the blob).
//! FORBIDDEN: serde_json::Value in return types.

use tauri::command;
use crate::ipc::m0_client::GoldenBlobJson;
use serde::{Deserialize, Serialize};

/// Coach findings returned to Cockpit — matches lineos-rule-engine CoachFindings.
/// All fields typed — no serde_json::Value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachFindingsJson {
    pub issues:         Vec<IssueJson>,
    pub recommendation: String,
}

/// A single finding — mirrors lineos-rule-engine Issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueJson {
    pub id:       String,
    pub severity: String,        // "info" | "low" | "medium" | "high"
    pub current:  f32,
    pub target:   f32,
    pub delta:    f32,
    pub tags:     Vec<String>,
}

/// Evaluate CoachFindings from a Golden Blob via the local rule-engine.
/// The rule-engine is a pure function — same blob + preset → identical findings.
/// Runs in-process (no M0 HTTP call) — the rule-engine is a native Rust crate.
#[command]
pub async fn evaluate_findings(blob: GoldenBlobJson) -> Result<CoachFindingsJson, String> {
    use lineos_rule_engine::{evaluate, AnalysisReport, QualityMetrics, ComplianceFlags, Thresholds};
    use sp314_dsp::types::config::Bmr128Schema;

    // Load schema — embedded at compile time for determinism
    let schema: Bmr128Schema = serde_json::from_str(
        include_str!("../../../../../lineos/shared/schema/bmr-128.schema.json")
    ).map_err(|e| format!("Schema parse error: {e}"))?;

    // Leak preset_id to &'static str — Box::leak is correct here:
    // The ThresholdsAPI requires &'static str for preset_name.
    // One small allocation per command invocation, acceptable.
    let preset_static: &'static str =
        Box::leak(blob.preset_id.clone().into_boxed_str());

    let thresholds = Thresholds::from_schema_with_preset(&schema, preset_static);

    let report = AnalysisReport::from_metrics(
        QualityMetrics {
            lufs_integrated:    blob.loudness.integrated_lufs,
            lufs_short_term:    blob.loudness.short_term_lufs,
            lufs_momentary:     blob.loudness.momentary_lufs,
            true_peak:          blob.loudness.true_peak_dbtp,
            loudness_range:     blob.loudness.lra,
            stereo_correlation: blob.quality.stereo_correlation,
            dynamic_range:      blob.quality.dynamic_range_db,
            dc_offset:          0.0,   // Phase 7: add dc_offset to blob spec
        },
        ComplianceFlags {
            spotify_ok:   blob.loudness.spotify_compliant,
            youtube_ok:   blob.loudness.youtube_compliant,
            apple_ok:     blob.loudness.apple_music_compliant,
            tidal_ok:     blob.loudness.tidal_compliant,
            broadcast_ok: blob.loudness.broadcast_compliant,
        },
    );

    let findings = evaluate(&report, &thresholds);

    // Convert lineos-rule-engine CoachFindings → CoachFindingsJson
    // IssueParams fields are already f32 typed — no serde_json::Value used.
    let issues = findings.issues.into_iter().map(|i| IssueJson {
        id:       i.id,
        severity: format!("{:?}", i.severity).to_lowercase(),
        current:  i.params.current,
        target:   i.params.target,
        delta:    i.params.delta,
        tags:     i.tags,
    }).collect();

    Ok(CoachFindingsJson {
        issues,
        recommendation: findings.recommendation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::m0_client::{LoudnessMetricsJson, QualityMetricsJson, ProvenanceJson};

    fn make_test_blob(preset: &str, lufs: f32) -> GoldenBlobJson {
        GoldenBlobJson {
            id:               "test-id".into(),
            version:          "1.0".into(),
            blob_type:        "audio".into(),
            created_at:       "2026-04-15T00:00:00Z".into(),
            input_hash:       "aabbccdd".into(),
            seed:             1,
            pipeline_version: "0.4.0".into(),
            preset_id:        preset.into(),
            loudness: LoudnessMetricsJson {
                integrated_lufs:          lufs,
                short_term_lufs:          lufs + 1.0,
                momentary_lufs:           lufs + 2.5,
                true_peak_dbtp:           -1.5,
                lra:                       8.0,
                k_weighted:               true,
                ebu_r128_target_lufs:     -23.0,
                ebu_r128_compliant:       false,
                spotify_compliant:        (lufs - (-14.0)).abs() < 1.0,
                youtube_compliant:        (lufs - (-14.0)).abs() < 1.0,
                apple_music_compliant:    (lufs - (-16.0)).abs() < 1.0,
                apple_podcasts_compliant: (lufs - (-16.0)).abs() < 1.0,
                broadcast_compliant:      (lufs - (-23.0)).abs() < 1.0,
                tidal_compliant:          (lufs - (-14.0)).abs() < 1.0,
            },
            quality: QualityMetricsJson {
                stereo_correlation: 0.94,
                phase_coherence:    0.97,
                stereo_width:       0.74,
                dynamic_range_db:   9.5,
                rms_db:             -16.0,
                spectral_centroid:  3_200.0,
                spectral_flatness:  0.12,
                clips_detected:     0,
                clip_free:          true,
            },
            provenance: ProvenanceJson {
                engine_id:          "E11".into(),
                engine_version:     "0.4.0".into(),
                processing_time_ms: 1_000,
                host_os:            "linux-x86_64".into(),
                created_by:         "test".into(),
                aether_enriched:    false,
                aether_devices:     vec![],
            },
        }
    }

    #[tokio::test]
    async fn test_evaluate_findings_spotify_too_loud() {
        // -12 LUFS with spotify preset (target -14) → lufs_compliance Medium
        let blob = make_test_blob("spotify", -12.0);
        let result = evaluate_findings(blob).await.unwrap();
        assert!(!result.issues.is_empty(), "Expected lufs_compliance finding");
        let lc = result.issues.iter().find(|i| i.id == "lufs_compliance");
        assert!(lc.is_some(), "lufs_compliance must be present");
        let lc = lc.unwrap();
        assert!((lc.delta - 2.0).abs() < 0.1, "delta should be ~2.0");
        assert!(result.recommendation.contains("Reduce gain"));
    }

    #[tokio::test]
    async fn test_evaluate_findings_clean_track() {
        // -14 LUFS with spotify preset → no issues
        let blob = make_test_blob("spotify", -14.0);
        let result = evaluate_findings(blob).await.unwrap();
        // No lufs_compliance — compliant track
        let lc = result.issues.iter().find(|i| i.id == "lufs_compliance");
        assert!(lc.is_none(), "Clean track should have no lufs_compliance issue");
    }
}
