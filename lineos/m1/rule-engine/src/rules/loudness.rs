//! Loudness rule — R001: lufs_compliance
//! Replaces three separate rules (lufs_too_high, lufs_too_low, lufs_apple_too_high).
//! Evaluates against the selected preset's target_lufs from Thresholds.
//! Severity scales with distance from target:
//!   delta.abs() > 2.0 → High
//!   delta.abs() > 1.0 → Medium
//!   else              → Low
//! Pure function: same input → same output. No side effects. No state.
//! Authority: Coach-Core README v1.1 §5.2

use crate::types::{AnalysisReport, Issue, IssueParams, Severity};
use crate::thresholds::Thresholds;

/// R001: Integrated LUFS compliance against the selected preset target.
///
/// Returns None if:
///   - `t.target_lufs` is None (raw preset — no LUFS obligation)
///   - `delta.abs() <= t.lufs_tolerance` (within the ±tolerance band)
///
/// Severity scales with distance from target:
///   delta.abs() > 2.0 → High   (significantly off-target; will be loudness-normalised hard)
///   delta.abs() > 1.0 → Medium (moderately off)  
///   else              → Low    (just outside tolerance; minor adjustment needed)
///
/// Tag: `platform:<preset_name>` — set by Cockpit's preset selection.
pub fn lufs_compliance(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    // Raw preset — no LUFS target, nothing to evaluate
    let target = match t.target_lufs {
        Some(v) => v,
        None    => return None,
    };

    let delta = r.quality.lufs_integrated - target;

    // Within tolerance band — compliant
    if delta.abs() <= t.lufs_tolerance {
        return None;
    }

    let severity = if delta.abs() > 2.0 {
        Severity::High
    } else if delta.abs() > 1.0 {
        Severity::Medium
    } else {
        Severity::Low
    };

    Some(Issue {
        id: "lufs_compliance".into(),
        severity,
        params: IssueParams {
            current: r.quality.lufs_integrated,
            target,
            delta,
        },
        tags: vec![format!("platform:{}", t.preset_name)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ComplianceFlags, QualityMetrics};

    /// Construct Thresholds with a specific preset_name and target_lufs.
    fn thresholds_for(preset: &'static str, target: Option<f32>) -> Thresholds {
        Thresholds {
            preset_name: preset,
            target_lufs: target,
            true_peak_max: -1.0,
            lufs_tolerance: 0.5,
            dynamic_range_min: 6.0,
            stereo_corr_min: 0.8,
            stereo_corr_warning: 0.5,
            dc_offset_max: 0.01,
            lra_max: 14.0,
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

    // ── No target (raw preset) ────────────────────────────────────────────────

    #[test]
    fn test_raw_preset_no_trigger() {
        let t = thresholds_for("raw", None);
        assert!(lufs_compliance(&report(-14.0), &t).is_none(),
            "raw preset (None target) must never trigger");
    }

    // ── Within tolerance — no trigger ─────────────────────────────────────────

    #[test]
    fn test_at_target_no_trigger() {
        let t = thresholds_for("spotify", Some(-14.0));
        assert!(lufs_compliance(&report(-14.0), &t).is_none(),
            "At target: delta=0.0, within tolerance");
    }

    #[test]
    fn test_at_tolerance_boundary_no_trigger() {
        // delta = -14.5 - (-14.0) = -0.5, abs = 0.5 = lufs_tolerance → no trigger (<=)
        let t = thresholds_for("spotify", Some(-14.0));
        assert!(lufs_compliance(&report(-14.5), &t).is_none(),
            "At exactly ±0.5 boundary: must not trigger");
    }

    #[test]
    fn test_just_inside_upper_tolerance_no_trigger() {
        // -13.51 → delta = 0.49 ≤ 0.5 → no trigger
        let t = thresholds_for("spotify", Some(-14.0));
        assert!(lufs_compliance(&report(-13.51), &t).is_none());
    }

    // ── Low severity (> tolerance, ≤ 1.0) ────────────────────────────────────

    #[test]
    fn test_just_outside_tolerance_is_low() {
        // -13.4 → delta = 0.6 > 0.5, abs ≤ 1.0 → Low
        let t = thresholds_for("spotify", Some(-14.0));
        let issue = lufs_compliance(&report(-13.4), &t).unwrap();
        assert_eq!(issue.severity, Severity::Low);
        assert_eq!(issue.id, "lufs_compliance");
    }

    #[test]
    fn test_below_target_is_low() {
        // -14.8 → delta = -0.8, abs = 0.8 > 0.5, ≤ 1.0 → Low
        let t = thresholds_for("spotify", Some(-14.0));
        let issue = lufs_compliance(&report(-14.8), &t).unwrap();
        assert_eq!(issue.severity, Severity::Low);
    }

    // ── Medium severity (> 1.0, ≤ 2.0) ──────────────────────────────────────

    #[test]
    fn test_delta_1_1_is_medium() {
        // -12.9 → delta = 1.1 > 1.0, ≤ 2.0 → Medium
        let t = thresholds_for("spotify", Some(-14.0));
        let issue = lufs_compliance(&report(-12.9), &t).unwrap();
        assert_eq!(issue.severity, Severity::Medium);
    }

    #[test]
    fn test_delta_2_0_is_medium() {
        // -12.0 → delta = 2.0, abs = 2.0 — NOT > 2.0 → Medium (boundary)
        let t = thresholds_for("spotify", Some(-14.0));
        let issue = lufs_compliance(&report(-12.0), &t).unwrap();
        assert_eq!(issue.severity, Severity::Medium,
            "delta=2.0 exactly: should be Medium (threshold is > 2.0)");
    }

    // ── High severity (> 2.0) ────────────────────────────────────────────────

    #[test]
    fn test_delta_2_1_is_high() {
        // -11.9 → delta = 2.1 > 2.0 → High
        let t = thresholds_for("spotify", Some(-14.0));
        let issue = lufs_compliance(&report(-11.9), &t).unwrap();
        assert_eq!(issue.severity, Severity::High);
    }

    #[test]
    fn test_large_delta_negative_is_high() {
        // -18.0 → delta = -4.0, abs = 4.0 > 2.0 → High
        let t = thresholds_for("spotify", Some(-14.0));
        let issue = lufs_compliance(&report(-18.0), &t).unwrap();
        assert_eq!(issue.severity, Severity::High);
    }

    // ── Apple Music preset ────────────────────────────────────────────────────

    #[test]
    fn test_apple_music_preset_at_target_no_trigger() {
        let t = thresholds_for("apple_music", Some(-16.0));
        assert!(lufs_compliance(&report(-16.0), &t).is_none());
    }

    #[test]
    fn test_apple_music_preset_triggers_with_correct_tag() {
        // -14.0 → delta from -16.0 = 2.0, abs = 2.0 → Medium
        let t = thresholds_for("apple_music", Some(-16.0));
        let issue = lufs_compliance(&report(-14.0), &t).unwrap();
        assert_eq!(issue.severity, Severity::Medium);
        assert!(issue.tags.iter().any(|tag| tag == "platform:apple_music"),
            "Tag must reflect preset: {:?}", issue.tags);
    }

    // ── Params correctness ────────────────────────────────────────────────────

    #[test]
    fn test_params_are_correct() {
        let t = thresholds_for("spotify", Some(-14.0));
        let issue = lufs_compliance(&report(-12.0), &t).unwrap();
        assert!((issue.params.current - (-12.0)).abs() < 0.001);
        assert!((issue.params.target  - (-14.0)).abs() < 0.001);
        assert!((issue.params.delta   -   2.0 ).abs() < 0.001);
    }

    #[test]
    fn test_tag_reflects_preset_name() {
        let t = thresholds_for("broadcast", Some(-23.0));
        let issue = lufs_compliance(&report(-20.0), &t).unwrap();
        assert!(issue.tags.iter().any(|tag| tag == "platform:broadcast"),
            "Tag must be platform:broadcast, got: {:?}", issue.tags);
    }
}
