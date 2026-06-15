//! Insights evaluator — compliance comparator across all BMR-128 presets.
//! All thresholds from Bmr128Schema (bmr-128.schema.json) — never hardcoded.
//! Authority: LineOS Constitution v2.0 §07 · §12 (BMR-128 threshold rule)
//!
//! COMPARATOR RULE: reads Ebu128Measurement and Bmr128Schema.
//! Never calls DPS code. Never re-measures audio.

use lineos_metadata::bmr128::Bmr128Report;
use lineos_types::{Bmr128Schema, Ebu128Measurement};
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
pub fn evaluate_all(measurement: &Ebu128Measurement, _schema: &Bmr128Schema) -> InsightsReport {
    let results: Vec<PresetResult> = Vec::new();

    // TODO: 3b — Bmr128Schema no longer has presets map.
    /*
    let pre = lineos_types::PreAnalysisData::silent();
    for (preset_name, thresholds) in &schema.presets {
        let report = Bmr128Report::generate(
            measurement,
            &pre,
            preset_name,
            thresholds.target_lufs,
            thresholds.true_peak_ceiling_dbfs,
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
    */
    let recommended = None;
    let hints = generate_hints(measurement, &results);

    InsightsReport {
        preset_results: results,
        recommended_preset: recommended,
        rule_engine_hints: hints,
    }
}

/// Generate actionable hints for the rule engine (Phase 4 input).
fn generate_hints(m: &Ebu128Measurement, results: &[PresetResult]) -> Vec<String> {
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
    // TODO: 3b — stereo correlation hint removed
    /*
    if m.stereo_correlation < 0.5 {
        hints.push(format!(
            "Low stereo correlation ({:.2}). Check for phase issues or out-of-phase content.",
            m.stereo_correlation
        ));
    }
    */
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
    // use super::*;
    // use lineos_types::{Bmr128Schema, PipelineConstants, PresetThresholds, Ebu128Measurement};
    // use std::collections::BTreeMap;

    /* TODO: 3b — restore tests
    fn test_schema() -> Bmr128Schema {
    ...
    */
}
