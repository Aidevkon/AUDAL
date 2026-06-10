//! Determinism + golden tests for lineos-rule-engine.
//! P4-006 requirement: same input ×100 → identical output.
//! v1.1: updated for lufs_compliance unified rule.
//! Authority: Coach-Core README v1.1 §11 · Phase 4 task-decomposition P4-006

use lineos_rule_engine::{
    evaluate, AnalysisReport, ComplianceFlags, QualityMetrics, Severity, Thresholds,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Standard Spotify preset thresholds.
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

// ── Determinism tests ────────────────────────────────────────────────────────

/// Core determinism requirement: same input ×100 → byte-identical output.
#[test]
fn test_determinism_100_runs() {
    let report = make_report(-12.0, -0.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();

    let result0 = evaluate(&report, &thresholds);
    for i in 0..100 {
        let result = evaluate(&report, &thresholds);
        assert_eq!(result, result0, "Determinism violation at run {i}");
    }
}

/// Determinism holds for a clean track.
#[test]
fn test_determinism_clean_track_100_runs() {
    // -14.0 LUFS is exactly on the Spotify target — no lufs_compliance trigger
    let report = make_report(-14.0, -1.5, 10.0, 0.95, 0.0, 8.0);
    let thresholds = make_thresholds();

    let result0 = evaluate(&report, &thresholds);
    for i in 0..100 {
        let result = evaluate(&report, &thresholds);
        assert_eq!(result, result0, "Determinism violation at run {i}");
    }
}

/// Determinism holds for worst-case (all rules triggered).
#[test]
fn test_determinism_all_rules_triggered_100_runs() {
    // >2 LU off target → High, TP exceeded, low DR, weak stereo, DC offset, high LRA
    let report = make_report(-11.0, 0.5, 4.0, 0.3, 0.1, 20.0);
    let thresholds = make_thresholds();

    let result0 = evaluate(&report, &thresholds);
    for i in 0..100 {
        let result = evaluate(&report, &thresholds);
        assert_eq!(result, result0, "Determinism violation at run {i}");
    }
}

/// Determinism holds across different preset configurations.
#[test]
fn test_determinism_broadcast_preset_100_runs() {
    let report = make_report(-14.0, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = Thresholds {
        preset_name: "broadcast",
        target_lufs: Some(-23.0),
        true_peak_max: -1.0,
        lufs_tolerance: 0.5,
        dynamic_range_min: 6.0,
        stereo_corr_min: 0.8,
        stereo_corr_warning: 0.5,
        dc_offset_max: 0.01,
        lra_max: 14.0,
    };

    let result0 = evaluate(&report, &thresholds);
    for i in 0..100 {
        let result = evaluate(&report, &thresholds);
        assert_eq!(result, result0, "Determinism violation at run {i}");
    }
}

// ── Golden tests ─────────────────────────────────────────────────────────────

/// Golden: true_peak_exceeded triggers with High severity + limiter recommendation.
#[test]
fn test_true_peak_exceeded_is_high_severity() {
    let report = make_report(-14.0, 0.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let issue = findings
        .issues
        .iter()
        .find(|i| i.id == "true_peak_exceeded");
    assert!(
        issue.is_some(),
        "true_peak_exceeded must trigger when TP = 0.5 > -1.0"
    );
    assert_eq!(issue.unwrap().severity, Severity::High);
    assert!(
        findings.recommendation.contains("limiter"),
        "Recommendation must reference limiter: {}",
        findings.recommendation
    );
    assert!(findings.has_blocking());
}

/// Golden: clean Spotify track (exactly at target) → no issues.
#[test]
fn test_clean_track_no_issues() {
    // -14.0 LUFS = Spotify target. delta = 0.0 ≤ 0.5 tolerance → no trigger.
    let report = make_report(-14.0, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    assert!(
        findings.issues.is_empty(),
        "Track at Spotify target must produce zero issues: {:?}",
        findings.issues
    );
    assert!(
        findings.recommendation.contains("ready"),
        "Recommendation must say 'ready': {}",
        findings.recommendation
    );
    assert!(!findings.has_blocking());
}

/// Golden: lufs_compliance triggers + "Reduce gain" when too loud.
#[test]
fn test_lufs_too_loud_triggers_compliance_and_reduce_recommendation() {
    // -12.0 → delta = 2.0 > 0.5 tolerance, severity = Medium (abs=2.0, not > 2.0)
    let report = make_report(-12.0, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let issue = findings.issues.iter().find(|i| i.id == "lufs_compliance");
    assert!(
        issue.is_some(),
        "lufs_compliance must trigger at -12 LUFS (Spotify target -14)"
    );
    assert_eq!(issue.unwrap().severity, Severity::Medium);
    assert!(
        findings.recommendation.contains("Reduce gain"),
        "Got: {}",
        findings.recommendation
    );
}

/// Golden: lufs_compliance triggers + "Increase gain" when too quiet.
#[test]
fn test_lufs_too_quiet_triggers_compliance_and_increase_recommendation() {
    // -20.0 → delta = -6.0, abs = 6.0 > 2.0 → High
    let report = make_report(-20.0, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let issue = findings.issues.iter().find(|i| i.id == "lufs_compliance");
    assert!(issue.is_some(), "lufs_compliance must trigger at -20 LUFS");
    assert_eq!(
        issue.unwrap().severity,
        Severity::High,
        "delta.abs()=6.0 > 2.0 → High"
    );
    assert!(
        findings.recommendation.contains("Increase gain"),
        "Got: {}",
        findings.recommendation
    );
}

/// Golden: true_peak_exceeded recommendation takes priority over lufs_compliance.
#[test]
fn test_true_peak_priority_over_lufs() {
    let report = make_report(-12.0, 0.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    assert!(
        findings.recommendation.contains("limiter"),
        "true_peak must dominate recommendation: {}",
        findings.recommendation
    );
}

/// Golden: lufs_compliance High fires when >2.0 LU from target.
#[test]
fn test_lufs_compliance_high_severity_above_2lu() {
    // -11.9 → delta = 2.1 > 2.0 → High
    let report = make_report(-11.9, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let issue = findings.issues.iter().find(|i| i.id == "lufs_compliance");
    assert!(issue.is_some(), "Must trigger at -11.9 LUFS");
    assert_eq!(
        issue.unwrap().severity,
        Severity::High,
        "delta.abs()=2.1 > 2.0 → High"
    );
}

/// Golden: raw preset never triggers lufs_compliance.
#[test]
fn test_raw_preset_no_lufs_compliance() {
    let report = make_report(-14.0, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = Thresholds {
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
    let findings = evaluate(&report, &thresholds);
    assert!(
        !findings.issues.iter().any(|i| i.id == "lufs_compliance"),
        "Raw preset must not produce lufs_compliance issue"
    );
}

/// Golden: weak stereo triggers High severity.
#[test]
fn test_weak_stereo_high_severity() {
    let report = make_report(-14.0, -1.5, 8.0, 0.3, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let issue = findings
        .issues
        .iter()
        .find(|i| i.id == "stereo_correlation_weak");
    assert!(
        issue.is_some(),
        "stereo_correlation_weak must trigger at corr=0.3"
    );
    assert_eq!(issue.unwrap().severity, Severity::High);
}

/// Golden: CoachFindings serializes to valid JSON with lowercase severity.
#[test]
fn test_findings_serialize_to_json() {
    let report = make_report(-12.0, 0.5, 4.0, 0.95, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let json = findings.to_json().expect("Serialization must succeed");
    assert!(json.contains("issues"));
    assert!(json.contains("recommendation"));
    assert!(
        !json.contains("\"High\""),
        "Severity must be lowercase: {json}"
    );
    assert!(json.contains("\"high\"") || json.contains("\"medium\"") || json.contains("\"low\""));
    // New rule id must appear, not old ids
    assert!(
        json.contains("lufs_compliance"),
        "Must use lufs_compliance id: {json}"
    );
    assert!(
        !json.contains("lufs_too_high"),
        "Old id must not appear: {json}"
    );
}

/// Boundary: lufs at exactly tolerance boundary — no trigger.
#[test]
fn test_lufs_exactly_at_tolerance_boundary() {
    // -13.5 → delta = 0.5, abs = 0.5 = tolerance → no trigger (condition is >, not >=)
    let report = make_report(-13.5, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    assert!(
        !findings.issues.iter().any(|i| i.id == "lufs_compliance"),
        "At exactly ±0.5 boundary: lufs_compliance must not trigger"
    );
}

/// Golden: Apple Music preset with correct tag in lufs_compliance issue.
#[test]
fn test_apple_music_preset_lufs_compliance_tag() {
    let report = make_report(-14.0, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = Thresholds {
        preset_name: "apple_music",
        target_lufs: Some(-16.0),
        true_peak_max: -1.0,
        lufs_tolerance: 0.5,
        dynamic_range_min: 6.0,
        stereo_corr_min: 0.8,
        stereo_corr_warning: 0.5,
        dc_offset_max: 0.01,
        lra_max: 14.0,
    };
    let findings = evaluate(&report, &thresholds);

    let issue = findings
        .issues
        .iter()
        .find(|i| i.id == "lufs_compliance")
        .unwrap();
    assert!(
        issue.tags.iter().any(|t| t == "platform:apple_music"),
        "Tag must say platform:apple_music: {:?}",
        issue.tags
    );
    // delta = -14.0 - (-16.0) = 2.0; abs = 2.0 → Medium (not > 2.0)
    assert_eq!(issue.severity, Severity::Medium);
}
