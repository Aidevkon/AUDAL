//! Stereo rules — R007, R008
//! R007: Stereo correlation weak or phase-inverted.
//! R008: DC offset detected.
//! All thresholds from &Thresholds (loaded from bmr-128.schema.json).

use crate::types::{AnalysisReport, Issue, IssueParams, Severity};
use crate::thresholds::Thresholds;

/// R007: Stereo correlation below threshold.
/// Two severity levels:
///   - < stereo_corr_warning (0.5): High — likely phase inversion, mono collapses
///   - < stereo_corr_min (0.8):     Medium — some mono compatibility risk
pub fn stereo_correlation_weak(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    let corr = r.quality.stereo_correlation;
    if corr < t.stereo_corr_warning {
        Some(Issue {
            id: "stereo_correlation_weak".into(),
            severity: Severity::High,
            params: IssueParams {
                current: corr,
                target:  t.stereo_corr_min,
                delta:   corr - t.stereo_corr_min,
            },
            tags: vec!["stereo".into(), "mono_compat".into()],
        })
    } else if corr < t.stereo_corr_min {
        Some(Issue {
            id: "stereo_correlation_low".into(),
            severity: Severity::Medium,
            params: IssueParams {
                current: corr,
                target:  t.stereo_corr_min,
                delta:   corr - t.stereo_corr_min,
            },
            tags: vec!["stereo".into()],
        })
    } else {
        None
    }
}

/// R008: DC offset detected in processed audio.
/// Severity: Medium — DC offset causes clicks, distortion, and DAC saturation.
pub fn dc_offset_detected(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    if r.quality.dc_offset.abs() > t.dc_offset_max {
        Some(Issue {
            id: "dc_offset_detected".into(),
            severity: Severity::Medium,
            params: IssueParams {
                current: r.quality.dc_offset,
                target:  0.0,
                delta:   r.quality.dc_offset.abs(),
            },
            tags: vec!["quality".into()],
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
            spotify_lufs: -14.0, youtube_lufs: -14.0, apple_lufs: -16.0,
            tidal_lufs: -14.0, broadcast_lufs: -23.0,
            true_peak_max: -1.0, lufs_tolerance: 0.5,
            dynamic_range_min: 6.0, stereo_corr_min: 0.8,
            stereo_corr_warning: 0.5, dc_offset_max: 0.01, lra_max: 14.0,
        }
    }

    fn report(corr: f32, dc: f32) -> AnalysisReport {
        AnalysisReport::from_metrics(
            QualityMetrics {
                lufs_integrated: -14.0, lufs_short_term: -13.0,
                lufs_momentary: -12.0, true_peak: -1.5,
                loudness_range: 8.0, stereo_correlation: corr,
                dynamic_range: 10.0, dc_offset: dc,
            },
            ComplianceFlags::all_pass(),
        )
    }

    #[test]
    fn test_stereo_corr_high_severity_below_warning() {
        let r = report(0.3, 0.0); // 0.3 < 0.5 → High
        let issue = stereo_correlation_weak(&r, &thresholds()).unwrap();
        assert_eq!(issue.id, "stereo_correlation_weak");
        assert_eq!(issue.severity, Severity::High);
    }

    #[test]
    fn test_stereo_corr_medium_severity_below_min() {
        let r = report(0.7, 0.0); // 0.5 ≤ 0.7 < 0.8 → Medium
        let issue = stereo_correlation_weak(&r, &thresholds()).unwrap();
        assert_eq!(issue.id, "stereo_correlation_low");
        assert_eq!(issue.severity, Severity::Medium);
    }

    #[test]
    fn test_stereo_corr_ok_no_trigger() {
        let r = report(0.9, 0.0); // 0.9 ≥ 0.8 → None
        assert!(stereo_correlation_weak(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_stereo_corr_at_min_no_trigger() {
        let r = report(0.8, 0.0); // exactly at min → no trigger
        assert!(stereo_correlation_weak(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_stereo_corr_at_warning_medium_not_high() {
        let r = report(0.5, 0.0); // exactly at warning → no trigger (>= required for High)
        assert!(stereo_correlation_weak(&r, &thresholds()).is_some());
        let issue = stereo_correlation_weak(&r, &thresholds()).unwrap();
        // 0.5 is not < 0.5, so falls to medium branch (0.5 < 0.8)
        assert_eq!(issue.severity, Severity::Medium);
    }

    #[test]
    fn test_dc_offset_triggers() {
        let r = report(0.95, 0.05); // 0.05 > 0.01 → trigger
        let issue = dc_offset_detected(&r, &thresholds());
        assert!(issue.is_some());
        assert_eq!(issue.unwrap().severity, Severity::Medium);
    }

    #[test]
    fn test_dc_offset_ok() {
        let r = report(0.95, 0.005); // 0.005 ≤ 0.01 → no trigger
        assert!(dc_offset_detected(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_dc_offset_negative_magnitude() {
        let r = report(0.95, -0.05); // negative offset also triggers
        assert!(dc_offset_detected(&r, &thresholds()).is_some());
    }
}
