//! Dynamics rules — R005, R006
//! R005: Dynamic range too low (over-compressed).
//! R006: LRA too high (excessive dynamics for streaming).
//! All thresholds from &Thresholds (loaded from bmr-128.schema.json).

use crate::thresholds::Thresholds;
use crate::types::{AnalysisReport, Issue, IssueParams, Severity};

/// R005: Dynamic range below minimum (over-compressed).
/// Severity: Low — informational; excessive compression reduces perceived quality.
pub fn dynamic_range_low(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    if r.quality.dynamic_range < t.dynamic_range_min {
        Some(Issue {
            id: "dynamic_range_low".into(),
            severity: Severity::Low,
            params: IssueParams {
                current: r.quality.dynamic_range,
                target: t.dynamic_range_min,
                delta: r.quality.dynamic_range - t.dynamic_range_min,
            },
            tags: vec!["dynamics".into()],
        })
    } else {
        None
    }
}

/// R006: Loudness Range (LRA) exceeds maximum (too dynamic for streaming).
/// Severity: Info — streaming platforms may apply further normalisation.
pub fn lra_too_high(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    if r.quality.loudness_range > t.lra_max {
        Some(Issue {
            id: "lra_too_high".into(),
            severity: Severity::Info,
            params: IssueParams {
                current: r.quality.loudness_range,
                target: t.lra_max,
                delta: r.quality.loudness_range - t.lra_max,
            },
            tags: vec!["dynamics".into(), "streaming".into()],
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

    fn report(dr: f32, lra: f32) -> AnalysisReport {
        AnalysisReport::from_metrics(
            QualityMetrics {
                lufs_integrated: -14.0,
                lufs_short_term: -13.0,
                lufs_momentary: -12.0,
                true_peak: -1.5,
                loudness_range: lra,
                stereo_correlation: 0.95,
                dynamic_range: dr,
                dc_offset: 0.0,
            },
            ComplianceFlags::all_pass(),
        )
    }

    #[test]
    fn test_dynamic_range_low_triggers() {
        let r = report(4.0, 8.0); // 4 < 6 → trigger
        assert!(dynamic_range_low(&r, &thresholds()).is_some());
    }

    #[test]
    fn test_dynamic_range_at_minimum_no_trigger() {
        let r = report(6.0, 8.0); // exactly at minimum → no trigger
        assert!(dynamic_range_low(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_dynamic_range_low_severity() {
        let r = report(3.0, 8.0);
        let issue = dynamic_range_low(&r, &thresholds()).unwrap();
        assert_eq!(issue.severity, Severity::Low);
    }

    #[test]
    fn test_lra_too_high_triggers() {
        let r = report(10.0, 16.0); // 16 > 14 → trigger
        let issue = lra_too_high(&r, &thresholds());
        assert!(issue.is_some());
        assert_eq!(issue.unwrap().severity, Severity::Info);
    }

    #[test]
    fn test_lra_at_max_no_trigger() {
        let r = report(10.0, 14.0); // exact boundary → no trigger
        assert!(lra_too_high(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_lra_tags_streaming() {
        let r = report(10.0, 20.0);
        let issue = lra_too_high(&r, &thresholds()).unwrap();
        assert!(issue.tags.iter().any(|t| t == "streaming"));
    }
}
