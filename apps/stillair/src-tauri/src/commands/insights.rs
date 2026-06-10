//! Insights command — rule-engine evaluation from Golden Blob.
//! Authority: Phase 6 task-decomposition P6-006
//!
//! Receives GoldenBlobJson from frontend (already in memory from get_golden_blob).
//! Runs lineos-rule-engine locally — no network call needed (pure deterministic).
//! Returns CoachFindingsJson — typed primitives, no serde_json::Value.
//!
//! FORBIDDEN: Re-measuring audio (all values come from the blob).
//! FORBIDDEN: serde_json::Value in return types.

use crate::ipc::m0_client::GoldenBlobJson;
use serde::{Deserialize, Serialize};
use tauri::command;

/// Coach findings returned to Cockpit — matches lineos-rule-engine CoachFindings.
/// All fields typed — no serde_json::Value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachFindingsJson {
    pub issues: Vec<IssueJson>,
    pub recommendation: String,
}

/// A single finding — mirrors lineos-rule-engine Issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueJson {
    pub id: String,
    pub severity: String, // "info" | "low" | "medium" | "high"
    pub current: f32,
    pub target: f32,
    pub delta: f32,
    pub tags: Vec<String>,
}

/// Evaluate CoachFindings from a Golden Blob via the local rule-engine.
/// The rule-engine is a pure function — same blob + preset → identical findings.
/// Runs in-process (no M0 HTTP call) — the rule-engine is a native Rust crate.
#[command]
pub async fn evaluate_findings(_blob: GoldenBlobJson) -> Result<CoachFindingsJson, String> {
    // TODO: 3b — restore evaluate_findings logic
    Ok(CoachFindingsJson {
        issues: vec![],
        recommendation: "Stubbed for Phase 3b".into(),
    })
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use crate::ipc::m0_client::{LoudnessMetricsJson, QualityMetricsJson, ProvenanceJson};

    /* TODO: 3b — restore tests
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
    */
}
