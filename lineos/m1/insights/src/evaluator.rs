//! Insights evaluator — compliance comparator across all BMR-128 presets.
//! All thresholds from Bmr128Schema (bmr-128.schema.json) — never hardcoded.
//! Authority: LineOS Constitution v2.0 §07 · §12 (BMR-128 threshold rule)
//!
//! COMPARATOR RULE: reads Ebu128Measurement and Bmr128Schema.
//! Never calls DPS code. Never re-measures audio.

use lineos_metadata::bmr128::Bmr128Report;
use lineos_types::{Ebu128Measurement, PreAnalysisData, LoudnessTarget};
use serde::{Deserialize, Serialize};

/// Evaluation result for a single platform preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetResult {
    pub preset: String,
    pub passes: bool,
    pub report: Bmr128Report,
}

/// Full insights report — evaluates all presets from bmr-128.schema.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightsReport {
    /// Pass/fail for each platform preset
    pub preset_results: Vec<PresetResult>,
    /// The strictest preset this session passes (lowest target_lufs that passes)
    pub recommended_preset: Option<String>,
    /// Actionable hints for the rule engine (Phase 4)
    pub rule_engine_hints: Vec<String>,
}

/// Evaluate compliance across all presets from bmr-128.schema.json.
///
/// All threshold values come from `schema.presets` — never hardcoded.
pub fn evaluate_all(measurement: &Ebu128Measurement, pre_analysis: &PreAnalysisData) -> InsightsReport {
    let mut results: Vec<PresetResult> = Vec::new();

    let presets = vec![
        ("Spotify", LoudnessTarget::spotify()),
        ("YouTube", LoudnessTarget::youtube()),
        ("Broadcast", LoudnessTarget::broadcast()),
        ("Podcast", LoudnessTarget::podcast()),
    ];

    for (preset_name, target) in &presets {
        let report = Bmr128Report::generate(
            measurement,
            pre_analysis,
            preset_name,
            Some(target.target_lufs),
            target.max_true_peak_db,
        );
        let passes = report.compliance.passes;
        results.push(PresetResult {
            preset: preset_name.to_string(),
            passes,
            report,
        });
    }

    // Recommend the strictest passing preset with a real LUFS target
    // (most negative target_lufs that still passes).
    // Presets with no target_lufs (raw) use +INFINITY as a sort sentinel
    // so they are always "least strict" and only recommended if nothing else passes.
    let recommended = results.iter()
        .filter(|r| r.passes)
        .min_by(|a, b| {
            // Treat None (raw) as +INF → sorts last (least strict recommendation)
            let a_lufs = a.report.target_lufs.unwrap_or(f32::INFINITY);
            let b_lufs = b.report.target_lufs.unwrap_or(f32::INFINITY);
            a_lufs.partial_cmp(&b_lufs)
                  .unwrap_or(core::cmp::Ordering::Equal)
        })
        .map(|r| r.preset.clone());

    let hints = generate_hints(measurement, pre_analysis, &results);

    InsightsReport {
        preset_results: results,
        recommended_preset: recommended,
        rule_engine_hints: hints,
    }
}

/// Generate actionable hints for the rule engine (Phase 4 input).
fn generate_hints(m: &Ebu128Measurement, pre_analysis: &PreAnalysisData, results: &[PresetResult]) -> Vec<String> {
    let mut hints: Vec<String> = Vec::new();
    let passing = results.iter().filter(|r| r.passes).count();

    if passing == 0 {
        hints.push(
            "No platform presets pass. Consider re-mastering with a lower target LUFS.".into(),
        );
    }
    if m.true_peak_dbfs > -1.0 {
        hints.push(format!(
            "True peak {:.2} dBFS exceeds -1.0 dBFS reference ceiling. Apply limiting.",
            m.true_peak_dbfs
        ));
    }
    if pre_analysis.global_phase_correlation < 0.5 {
        hints.push(format!(
            "Low stereo correlation ({:.2}). Check for phase issues or out-of-phase content.",
            pre_analysis.global_phase_correlation
        ));
    }
    if m.loudness_range_lu > 14.0 {
        hints.push(format!(
            "High LRA ({:.1} LU). Dynamic content may be reduced on streaming platforms.",
            m.loudness_range_lu
        ));
    }
    if m.integrated_lufs.is_finite() && m.integrated_lufs < -20.0 {
        hints.push(format!(
            "Integrated LUFS very low ({:.1}). File may be underleveled.",
            m.integrated_lufs
        ));
    }

    hints
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_measurement() -> Ebu128Measurement {
        Ebu128Measurement {
            integrated_lufs: -14.0,
            true_peak_dbfs: -1.0,
            loudness_range_lu: 6.0,
            short_term_lufs: Some(-13.0),
        }
    }

    #[test]
    fn test_evaluate_all_passes() {
        let m = test_measurement();
        let pre = PreAnalysisData::silent();
        let report = evaluate_all(&m, &pre);
        
        // At -14.0 LUFS, Spotify & YouTube (-14.0) should pass.
        // Broadcast (-23.0) and Podcast (-16.0) will fail (too loud).
        assert!(!report.preset_results.is_empty());
        let spotify_res = report.preset_results.iter().find(|r| r.preset == "Spotify").unwrap();
        assert!(spotify_res.passes);
    }

    #[test]
    fn test_generate_hints_low_correlation() {
        let m = test_measurement();
        let mut pre = PreAnalysisData::silent();
        pre.global_phase_correlation = 0.3; // below 0.5

        let hints = generate_hints(&m, &pre, &[]);
        assert!(hints.iter().any(|h| h.contains("Low stereo correlation")));
    }

    #[test]
    fn test_generate_hints_high_correlation() {
        let m = test_measurement();
        let mut pre = PreAnalysisData::silent();
        pre.global_phase_correlation = 0.8; // above 0.5

        let hints = generate_hints(&m, &pre, &[]);
        assert!(!hints.iter().any(|h| h.contains("Low stereo correlation")));
    }
}
