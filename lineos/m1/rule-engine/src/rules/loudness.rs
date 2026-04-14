//! Loudness rules — R001, R002, R003
//! Pure functions: same input → same output. No side effects. No state.
//! All thresholds from &Thresholds (loaded from bmr-128.schema.json).

use crate::types::{AnalysisReport, Issue, IssueParams, Severity};
use crate::thresholds::Thresholds;

/// R001: Integrated LUFS above Spotify/YouTube target (+ tolerance band).
/// Severity: Medium — streaming normalisation will reduce perceived volume.
pub fn lufs_too_high(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    if r.quality.lufs_integrated > t.spotify_lufs + t.lufs_tolerance {
        Some(Issue {
            id: "lufs_too_high".into(),
            severity: Severity::Medium,
            params: IssueParams {
                current: r.quality.lufs_integrated,
                target:  t.spotify_lufs,
                delta:   r.quality.lufs_integrated - t.spotify_lufs,
            },
            tags: vec!["platform:spotify".into(), "platform:youtube".into()],
        })
    } else {
        None
    }
}

/// R002: Integrated LUFS below Spotify target (too quiet).
/// Severity: Low — streaming may boost, but perceptual quality suffers.
pub fn lufs_too_low(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    if r.quality.lufs_integrated < t.spotify_lufs - t.lufs_tolerance {
        Some(Issue {
            id: "lufs_too_low".into(),
            severity: Severity::Low,
            params: IssueParams {
                current: r.quality.lufs_integrated,
                target:  t.spotify_lufs,
                delta:   r.quality.lufs_integrated - t.spotify_lufs,
            },
            tags: vec!["platform:spotify".into()],
        })
    } else {
        None
    }
}

/// R003: Integrated LUFS above Apple Music target (-16 LUFS, stricter).
/// Severity: Low — Apple Music has a lower normalisation target.
pub fn lufs_apple_too_high(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    if r.quality.lufs_integrated > t.apple_lufs + t.lufs_tolerance {
        Some(Issue {
            id: "lufs_apple_too_high".into(),
            severity: Severity::Low,
            params: IssueParams {
                current: r.quality.lufs_integrated,
                target:  t.apple_lufs,
                delta:   r.quality.lufs_integrated - t.apple_lufs,
            },
            tags: vec!["platform:apple_music".into()],
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

    fn report(lufs: f32) -> AnalysisReport {
        AnalysisReport::from_metrics(
            QualityMetrics {
                lufs_integrated: lufs, lufs_short_term: lufs + 1.0,
                lufs_momentary: lufs + 2.0, true_peak: -1.5,
                loudness_range: 8.0, stereo_correlation: 0.95,
                dynamic_range: 10.0, dc_offset: 0.0,
            },
            ComplianceFlags::all_pass(),
        )
    }

    #[test]
    fn test_lufs_too_high_triggers_above_tolerance() {
        let r = report(-12.0); // -12 > -14 + 0.5 = -13.5
        assert!(lufs_too_high(&r, &thresholds()).is_some());
    }

    #[test]
    fn test_lufs_too_high_no_trigger_within_band() {
        let r = report(-14.0); // exactly at target → no trigger
        assert!(lufs_too_high(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_lufs_too_low_triggers() {
        let r = report(-16.0); // -16 < -14 - 0.5 = -14.5
        assert!(lufs_too_low(&r, &thresholds()).is_some());
    }

    #[test]
    fn test_lufs_too_low_no_trigger_within_band() {
        let r = report(-14.0);
        assert!(lufs_too_low(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_lufs_apple_too_high_triggers() {
        let r = report(-14.0); // -14 > -16 + 0.5 = -15.5
        let issue = lufs_apple_too_high(&r, &thresholds());
        assert!(issue.is_some());
        assert_eq!(issue.unwrap().severity, Severity::Low);
    }

    #[test]
    fn test_lufs_apple_ok_at_target() {
        let r = report(-16.0); // exactly at Apple target
        assert!(lufs_apple_too_high(&r, &thresholds()).is_none());
    }

    #[test]
    fn test_lufs_too_high_params_correct() {
        let r = report(-12.0);
        let issue = lufs_too_high(&r, &thresholds()).unwrap();
        assert!((issue.params.current - (-12.0)).abs() < 0.001);
        assert!((issue.params.target - (-14.0)).abs() < 0.001);
        assert!((issue.params.delta - 2.0).abs() < 0.001);
    }
}
