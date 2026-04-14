//! Peak rules — R004
//! R004: True peak exceeded ceiling.
//! Severity: High (blocking) — must not export until resolved.
//! All thresholds from &Thresholds (loaded from bmr-128.schema.json).

use crate::types::{AnalysisReport, Issue, IssueParams, Severity};
use crate::thresholds::Thresholds;

/// R004: True peak exceeds the ceiling from bmr-128.schema.json.
/// This is a BLOCKING issue (High severity). Track must be re-limited.
pub fn true_peak_exceeded(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    if r.quality.true_peak > t.true_peak_max {
        Some(Issue {
            id: "true_peak_exceeded".into(),
            severity: Severity::High,
            params: IssueParams {
                current: r.quality.true_peak,
                target:  t.true_peak_max,
                delta:   r.quality.true_peak - t.true_peak_max,
            },
            tags: vec!["compliance:critical".into()],
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ComplianceFlags, QualityMetrics};

    fn thresholds() -> Thresholds {
        Thresholds {
            preset_name: "spotify", target_lufs: Some(-14.0),
            true_peak_max: -1.0, lufs_tolerance: 0.5,
            dynamic_range_min: 6.0, stereo_corr_min: 0.8,
            stereo_corr_warning: 0.5, dc_offset_max: 0.01, lra_max: 14.0,
        }
    }

    fn report(tp: f32) -> AnalysisReport {
        AnalysisReport::from_metrics(
            QualityMetrics {
                lufs_integrated: -14.0, lufs_short_term: -13.0,
                lufs_momentary: -12.0, true_peak: tp,
                loudness_range: 8.0, stereo_correlation: 0.95,
                dynamic_range: 10.0, dc_offset: 0.0,
            },
            ComplianceFlags::all_pass(),
        )
    }

    #[test]
    fn test_true_peak_exceeded_triggers() {
        let r = report(0.5); // 0.5 > -1.0 → trigger
        let issue = true_peak_exceeded(&r, &thresholds());
        assert!(issue.is_some());
        assert_eq!(issue.unwrap().severity, Severity::High);
    }

    #[test]
    fn test_true_peak_at_ceiling_no_trigger() {
        let r = report(-1.0); // exactly at ceiling → no trigger
        assert!(true_peak_exceeded(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_true_peak_below_ceiling_no_trigger() {
        let r = report(-2.0);
        assert!(true_peak_exceeded(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_true_peak_params_correct() {
        let r = report(-0.5); // -0.5 > -1.0
        let issue = true_peak_exceeded(&r, &thresholds()).unwrap();
        assert!((issue.params.current - (-0.5)).abs() < 0.001);
        assert!((issue.params.target - (-1.0)).abs() < 0.001);
        assert!((issue.params.delta - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_true_peak_tags_critical() {
        let r = report(1.0);
        let issue = true_peak_exceeded(&r, &thresholds()).unwrap();
        assert!(issue.tags.iter().any(|t| t == "compliance:critical"));
    }
}
