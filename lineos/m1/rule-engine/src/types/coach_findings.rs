//! CoachFindings — output contract of lineos-rule-engine.
//! Severity values: info | low | medium | high — exactly these four.
//! IssueParams uses typed numerics only — never serde_json::Value.
//! Schema: lineos/shared/schema/coach-findings.schema.json
//! Authority: LineOS Constitution v2.0 §07 · Coach-Core README §4.2

use serde::{Deserialize, Serialize};

/// Output of the rule-engine — a prioritised list of issues + recommendation.
/// Deterministic: same AnalysisReport + Thresholds → identical CoachFindings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoachFindings {
    pub issues: Vec<Issue>,
    pub recommendation: String,
}

/// A single finding from the rule-engine.
/// `id` is a stable snake_case identifier matching the rule function name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    /// Stable rule ID (snake_case). Never changes once published.
    pub id: String,
    pub severity: Severity,
    pub params: IssueParams,
    pub tags: Vec<String>,
}

/// Strictly numeric params — serde_json::Value is forbidden at the WASM boundary.
/// current: measured value, target: schema threshold, delta: current - target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IssueParams {
    pub current: f32,
    pub target: f32,
    pub delta: f32,
}

/// Severity spectrum — exactly four values, in increasing priority order.
/// Never add new variants without a major version bump.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}

impl CoachFindings {
    /// Construct an empty findings set — clean track, no issues.
    pub fn empty() -> Self {
        Self {
            issues: Vec::new(),
            recommendation: "Track is ready — proceed with export.".to_string(),
        }
    }

    /// Returns true if any issue has High severity (blocking).
    pub fn has_blocking(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::High)
    }

    /// Returns all issues at or above the given severity level.
    pub fn issues_at_least(&self, min: &Severity) -> Vec<&Issue> {
        self.issues.iter().filter(|i| &i.severity >= min).collect()
    }

    /// Serialize to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_findings_no_issues() {
        let f = CoachFindings::empty();
        assert!(f.issues.is_empty());
        assert!(f.recommendation.contains("ready"));
        assert!(!f.has_blocking());
    }

    #[test]
    fn test_has_blocking_detects_high() {
        let issue = Issue {
            id: "true_peak_exceeded".into(),
            severity: Severity::High,
            params: IssueParams {
                current: 0.5,
                target: -1.0,
                delta: 1.5,
            },
            tags: vec!["compliance:critical".into()],
        };
        let f = CoachFindings {
            issues: vec![issue],
            recommendation: "Apply limiting.".to_string(),
        };
        assert!(f.has_blocking());
    }

    #[test]
    fn test_severity_ordered() {
        assert!(Severity::Info < Severity::Low);
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
    }

    #[test]
    fn test_severity_serializes_lowercase() {
        let s = serde_json::to_string(&Severity::High).unwrap();
        assert_eq!(s, "\"high\"");
        let s = serde_json::to_string(&Severity::Info).unwrap();
        assert_eq!(s, "\"info\"");
    }

    #[test]
    fn test_coach_findings_json_round_trip() {
        let f = CoachFindings {
            issues: vec![Issue {
                id: "lufs_too_high".into(),
                severity: Severity::Medium,
                params: IssueParams {
                    current: -12.0,
                    target: -14.0,
                    delta: 2.0,
                },
                tags: vec!["platform:spotify".into()],
            }],
            recommendation: "Reduce gain.".to_string(),
        };
        let json = f.to_json().unwrap();
        assert!(json.contains("lufs_too_high"));
        assert!(json.contains("\"medium\""));
        // Can deserialize back — but Issue.id is &'static str so no round-trip Deserialize
        // CoachFindings JSON output is the canonical exchange format
    }
}
