//! Evaluator — pure evaluation pipeline.
//! evaluate() is the single entry point for the rule-engine.
//! Pure function: same AnalysisReport + Thresholds → identical CoachFindings.
//! O(N) where N = RULES.len() = 6. Each rule is O(1). Zero heap alloc in hot path.
//! No LLM, no randomness, no side effects, no global state.
//! Authority: LineOS Constitution v2.0 §07 · Coach-Core README v1.1 §2, §7

use crate::rules::registry::RULES;
use crate::thresholds::Thresholds;
use crate::types::{AnalysisReport, CoachFindings, Issue, Severity};

/// Evaluate all rules against the analysis report.
/// Returns CoachFindings with all triggered issues and a deterministic recommendation.
pub fn evaluate(report: &AnalysisReport, thresholds: &Thresholds) -> CoachFindings {
    let issues: Vec<Issue> = RULES
        .iter()
        .filter_map(|rule| (rule.check)(report, thresholds))
        .collect();

    let recommendation = derive_recommendation(&issues);

    CoachFindings {
        issues,
        recommendation,
    }
}

/// Deterministic recommendation derived from the issue list.
/// Priority order (first match wins):
///   1. true_peak_exceeded               → specific limiter instruction
///   2. Any High severity *other than TP* → critical review
///   3. lufs_compliance with delta > 0   → too loud, reduce gain
///   4. lufs_compliance with delta < 0   → too quiet, increase gain
///   5. Any Medium severity              → medium review
///   6. No issues                        → ready for export
///   7. Info/Low only                    → minor review
///
/// Same issues → same string. Always. (Determinism guarantee.)
fn derive_recommendation(issues: &[Issue]) -> String {
    // 1. True peak is always blocking — specific action required
    if issues.iter().any(|i| i.id == "true_peak_exceeded") {
        return "Apply brick-wall limiter — true peak exceeds ceiling.".to_string();
    }
    // 2. lufs_compliance — checked before generic High catch-all so gain direction is clear
    //    delta > 0: track is too loud relative to target
    //    delta < 0: track is too quiet relative to target
    if let Some(lc) = issues.iter().find(|i| i.id == "lufs_compliance") {
        if lc.params.delta > 0.0 {
            return "Reduce gain to meet loudness target.".to_string();
        } else {
            return "Increase gain to meet loudness target.".to_string();
        }
    }
    // 3. Other High-severity issues (stereo, etc.)
    if issues.iter().any(|i| i.severity == Severity::High) {
        return "Critical issues detected — review findings before export.".to_string();
    }
    // 4. Medium-priority
    if issues.iter().any(|i| i.severity == Severity::Medium) {
        return "Medium-priority issues detected — review findings.".to_string();
    }
    // 5. No issues
    if issues.is_empty() {
        return "Track is ready — proceed with export.".to_string();
    }
    // 6. Info/Low only
    "Minor issues detected — review findings.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ComplianceFlags, QualityMetrics};

    fn make_thresholds() -> Thresholds {
        Thresholds {
            preset_name: "spotify",
            target_lufs: Some(-14.0),
            true_peak_max: -1.0,
            lufs_tolerance: 0.5,
            dynamic_range_min: 6.0,
            stereo_corr_min: 0.8,
            stereo_corr_warning: 0.5,
            dc_offset_max: 0.01,
            lra_max: 14.0,
        }
    }

    fn make_report(lufs: f32, tp: f32, dr: f32, corr: f32, dc: f32, lra: f32) -> AnalysisReport {
        AnalysisReport::from_metrics(
            QualityMetrics {
                lufs_integrated: lufs,
                lufs_short_term: lufs + 1.0,
                lufs_momentary: lufs + 2.0,
                true_peak: tp,
                loudness_range: lra,
                stereo_correlation: corr,
                dynamic_range: dr,
                dc_offset: dc,
            },
            ComplianceFlags::all_pass(),
        )
    }

    #[test]
    fn test_clean_track_no_issues() {
        // -14.0 LUFS, within Spotify ±0.5 tolerance, all other metrics clean
        let r = make_report(-14.0, -1.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(
            f.issues.is_empty(),
            "Track at target LUFS must have no issues: {:?}",
            f.issues
        );
        assert!(f.recommendation.contains("ready"));
    }

    #[test]
    fn test_true_peak_exceeded_recommendation() {
        let r = make_report(-14.0, 0.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(f.recommendation.contains("limiter"));
    }

    #[test]
    fn test_lufs_too_loud_recommendation() {
        // -12.0 LUFS → delta = 2.0, above spotify target. Severity Medium (delta.abs()=2.0 not > 2.0)
        let r = make_report(-12.0, -1.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(
            f.recommendation.contains("Reduce gain"),
            "Expected Reduce gain: {}",
            f.recommendation
        );
    }

    #[test]
    fn test_lufs_too_quiet_recommendation() {
        // -20.0 LUFS → delta = -6.0, below target. High severity.
        let r = make_report(-20.0, -1.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(
            f.recommendation.contains("Increase gain"),
            "Expected Increase gain: {}",
            f.recommendation
        );
    }

    #[test]
    fn test_all_issues_collected() {
        // All 6 rules trigger
        let r = make_report(-11.0, 0.5, 4.0, 0.3, 0.1, 20.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(!f.issues.is_empty());
        assert!(f.issues.iter().any(|i| i.id == "true_peak_exceeded"));
        assert!(f.recommendation.contains("limiter"));
    }

    #[test]
    fn test_lufs_compliance_issue_has_correct_id() {
        let r = make_report(-12.0, -1.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &make_thresholds());
        assert!(
            f.issues.iter().any(|i| i.id == "lufs_compliance"),
            "Must use id 'lufs_compliance', not old ids: {:?}",
            f.issues
        );
    }

    #[test]
    fn test_raw_preset_no_lufs_issue() {
        let t = Thresholds {
            preset_name: "raw",
            target_lufs: None,
            true_peak_max: -0.1,
            lufs_tolerance: 0.5,
            dynamic_range_min: 6.0,
            stereo_corr_min: 0.8,
            stereo_corr_warning: 0.5,
            dc_offset_max: 0.01,
            lra_max: 14.0,
        };
        let r = make_report(-14.0, -1.5, 10.0, 0.95, 0.0, 8.0);
        let f = evaluate(&r, &t);
        assert!(
            !f.issues.iter().any(|i| i.id == "lufs_compliance"),
            "Raw preset must produce no lufs_compliance issue"
        );
    }
}
