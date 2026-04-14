//! Determinism + golden tests for lineos-rule-engine.
//! P4-006 requirement: same input ×100 → identical output.
//! Authority: Coach-Core README §11 · Phase 4 task-decomposition P4-006

use lineos_rule_engine::{
    evaluate, AnalysisReport, ComplianceFlags, QualityMetrics, Severity, Thresholds,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

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

/// Determinism holds for a clean track (no issues).
#[test]
fn test_determinism_clean_track_100_runs() {
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
    let report = make_report(-12.0, 0.5, 4.0, 0.3, 0.1, 20.0);
    let thresholds = make_thresholds();

    let result0 = evaluate(&report, &thresholds);
    for i in 0..100 {
        let result = evaluate(&report, &thresholds);
        assert_eq!(result, result0, "Determinism violation at run {i}");
    }
}

// ── Golden tests ─────────────────────────────────────────────────────────────

/// Golden: true_peak_exceeded triggers with High severity, recommendation contains "limiter".
#[test]
fn test_true_peak_exceeded_is_high_severity() {
    let report = make_report(-14.0, 0.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let issue = findings.issues.iter().find(|i| i.id == "true_peak_exceeded");
    assert!(issue.is_some(), "true_peak_exceeded rule must trigger when TP = 0.5 > -1.0");
    assert_eq!(issue.unwrap().severity, Severity::High);
    assert!(findings.recommendation.contains("limiter"),
        "Recommendation must reference limiter: {}", findings.recommendation);
    assert!(findings.has_blocking());
}

/// Golden: Spotify-compliant track has no blocking issues and no lufs_too_high.
/// Note: lufs_apple_too_high (Low) fires correctly — Apple and Spotify targets diverge.
#[test]
fn test_clean_track_no_issues() {
    // -14.0 LUFS: within Spotify window (±0.5). Apple low severity fires (expected).
    let report = make_report(-14.0, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    assert!(!findings.has_blocking(),
        "Spotify-compliant track must have no blocking issues: {:?}", findings.issues);
    assert!(!findings.issues.iter().any(|i| i.id == "true_peak_exceeded"),
        "true_peak_exceeded must not fire: {:?}", findings.issues);
    assert!(!findings.issues.iter().any(|i| i.id == "lufs_too_high"),
        "lufs_too_high must not fire at -14.0: {:?}", findings.issues);
}

/// Golden: LUFS too high triggers Medium severity and "Reduce gain" recommendation.
#[test]
fn test_lufs_too_high_medium_severity() {
    let report = make_report(-12.0, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let issue = findings.issues.iter().find(|i| i.id == "lufs_too_high");
    assert!(issue.is_some(), "lufs_too_high must trigger at -12 LUFS");
    assert_eq!(issue.unwrap().severity, Severity::Medium);
    assert!(findings.recommendation.contains("Reduce gain"));
}

/// Golden: LUFS too low triggers Low severity and "Increase gain" recommendation.
#[test]
fn test_lufs_too_low_low_severity() {
    let report = make_report(-20.0, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let issue = findings.issues.iter().find(|i| i.id == "lufs_too_low");
    assert!(issue.is_some(), "lufs_too_low must trigger at -20 LUFS");
    assert_eq!(issue.unwrap().severity, Severity::Low);
    assert!(findings.recommendation.contains("Increase gain"));
}

/// Golden: true_peak_exceeded recommendation takes priority over LUFS issues.
#[test]
fn test_true_peak_priority_over_lufs() {
    // Both lufs_too_high and true_peak_exceeded should trigger
    let report = make_report(-12.0, 0.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    // true_peak priority must win
    assert!(findings.recommendation.contains("limiter"),
        "true_peak should dominate recommendation: {}", findings.recommendation);
}

/// Golden: weak stereo triggers High severity (stereo_correlation_weak).
#[test]
fn test_weak_stereo_high_severity() {
    let report = make_report(-14.0, -1.5, 8.0, 0.3, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let issue = findings.issues.iter().find(|i| i.id == "stereo_correlation_weak");
    assert!(issue.is_some(), "stereo_correlation_weak must trigger at corr=0.3");
    assert_eq!(issue.unwrap().severity, Severity::High);
}

/// Golden: CoachFindings serializes to valid JSON.
#[test]
fn test_findings_serialize_to_json() {
    let report = make_report(-12.0, 0.5, 4.0, 0.95, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let json = findings.to_json().expect("Serialization must succeed");
    assert!(json.contains("issues"));
    assert!(json.contains("recommendation"));
    // Severity must be lowercase per schema
    assert!(!json.contains("\"High\""), "Severity must be lowercase: {json}");
    assert!(json.contains("\"high\"") || json.contains("\"medium\"") || json.contains("\"low\""));
}

/// Boundary test: LUFS exactly at tolerance boundary — no trigger.
#[test]
fn test_lufs_exactly_at_tolerance_boundary() {
    // lufs_too_high triggers only when > spotify_lufs + tolerance = -13.5
    let report = make_report(-13.5, -1.5, 8.0, 0.9, 0.0, 5.0);
    let thresholds = make_thresholds();
    let findings = evaluate(&report, &thresholds);

    let has_too_high = findings.issues.iter().any(|i| i.id == "lufs_too_high");
    assert!(!has_too_high, "LUFS at exactly tolerance boundary (-13.5) must not trigger");
}
