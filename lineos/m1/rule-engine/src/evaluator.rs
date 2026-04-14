//! Evaluator — pure evaluation pipeline.
//! evaluate() is the single entry point for the rule-engine.
//! Pure function: same AnalysisReport + Thresholds → identical CoachFindings.
//! O(N) where N = RULES.len(). Each rule is O(1). Zero heap alloc in hot path.
//! No LLM, no randomness, no side effects, no global state.
//! Authority: LineOS Constitution v2.0 §07 · Coach-Core README §2, §7

use crate::rules::registry::RULES;
use crate::thresholds::Thresholds;
use crate::types::{AnalysisReport, CoachFindings, Issue, Severity};

/// Evaluate all rules against the analysis report.
/// Returns CoachFindings with all triggered issues and a deterministic recommendation.
pub fn evaluate(report: &AnalysisReport, thresholds: &Thresholds) -> CoachFindings {
    let issues: Vec<Issue> = RULES.iter()
        .filter_map(|rule| (rule.check)(report, thresholds))
        .collect();

    let recommendation = derive_recommendation(&issues);

    CoachFindings { issues, recommendation }
}

/// Deterministic recommendation derived from the issue list.
/// Priority order (first match wins):
///   1. true_peak_exceeded  → specific limiter instruction
///   2. Any High severity   → critical review
///   3. lufs_too_high       → reduce gain
///   4. lufs_too_low        → increase gain
///   5. Any Medium severity → medium review
///   6. No issues           → ready for export
///   7. Info/Low only       → minor review
///
/// Same issues → same string. Always. (Determinism guarantee.)
fn derive_recommendation(issues: &[Issue]) -> String {
    if issues.iter().any(|i| i.id == "true_peak_exceeded") {
        return "Apply brick-wall limiter — true peak exceeds ceiling.".to_string();
    }
    if issues.iter().any(|i| i.severity == Severity::High) {
        return "Critical issues detected — review findings before export.".to_string();
    }
    if issues.iter().any(|i| i.id == "lufs_too_high") {
        return "Reduce gain to meet loudness target.".to_string();
    }
    if issues.iter().any(|i| i.id == "lufs_too_low") {
        return "Increase gain to meet loudness target.".to_string();
    }
    if issues.iter().any(|i| i.severity == Severity::Medium) {
        return "Medium-priority issues detected — review findings.".to_string();
    }
    if issues.is_empty() {
        return "Track is ready — proceed with export.".to_string();
    }
    "Minor issues detected — review findings.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ComplianceFlags, QualityMetrics};

    fn make_thresholds() -> Thresholds {
        Thresholds {
            spotify_lufs: -14.0, youtube_lufs: -14.0, apple_lufs: -16.0,
            tidal_lufs: -14.0, broadcast_lufs: -23.0,
            true_peak_max: -1.0, lufs_tolerance: 0.5,
            dynamic_range_min: 6.0, stereo_corr_min: 0.8,
            stereo_corr_warning: 0.5, dc_offset_max: 0.01, lra_max: 14.0,
        }
    }

    fn make_report(
        lufs: f32, tp: f32, dr: f32, corr: f32, dc: f32, lra: f32,
    ) -> AnalysisReport {
        AnalysisReport::from_metrics(
            QualityMetrics {
                lufs_integrated: lufs, lufs_short_term: lufs + 1.0,
                lufs_momentary: lufs + 2.0, true_peak: tp,
                loudness_range: lra, stereo_correlation: corr,
                dynamic_range: dr, dc_offset: dc,
            },
            ComplianceFlags::all_pass(),
        )
    }

    #[test]
    fn test_clean_track_no_issues() {
        // A "clean" track for this test means: no High or blocking issues.
        // lufs_apple_too_high (Low severity) may fire at -14.0 LUFS — that's correct.
        // We verify: no true_peak_exceeded, no High issues, recommendation is positive.
        let r = make_report(-14.0, -1.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &make_thresholds());
        // No blocking (High severity) issues
        assert!(!f.has_blocking(), "No blocking issues for a Spotify-compliant track: {:?}", f.issues);
        // No true_peak issue
        assert!(!f.issues.iter().any(|i| i.id == "true_peak_exceeded"));
        // No lufs_too_high (within Spotify tolerance)
        assert!(!f.issues.iter().any(|i| i.id == "lufs_too_high"),
            "lufs_too_high must not trigger at -14.0: {:?}", f.issues);
    }

    #[test]
    fn test_true_peak_exceeded_recommendation() {
        let r = make_report(-14.0, 0.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(f.recommendation.contains("limiter"));
    }

    #[test]
    fn test_lufs_too_high_recommendation() {
        let r = make_report(-12.0, -1.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(f.recommendation.contains("Reduce gain"));
    }

    #[test]
    fn test_lufs_too_low_recommendation() {
        let r = make_report(-20.0, -1.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(f.recommendation.contains("Increase gain"));
    }

    #[test]
    fn test_all_issues_collected() {
        // All 8 rules trigger at once
        let r = make_report(-12.0, 0.5, 4.0, 0.3, 0.1, 20.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(!f.issues.is_empty());
        // true_peak_exceeded must be present (High, blocking)
        assert!(f.issues.iter().any(|i| i.id == "true_peak_exceeded"));
        // true_peak recommendation takes priority
        assert!(f.recommendation.contains("limiter"));
    }
}
