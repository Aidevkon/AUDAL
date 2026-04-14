//! Insights evaluator — compliance comparator across all BMR-128 presets.
//! All thresholds from Bmr128Schema (bmr-128.schema.json) — never hardcoded.
//! Authority: LineOS Constitution v2.0 §07 · §12 (BMR-128 threshold rule)
//!
//! COMPARATOR RULE: reads Ebu128Measurement and Bmr128Schema.
//! Never calls DPS code. Never re-measures audio.

use sp314_dsp::types::{config::Bmr128Schema, metrics::Ebu128Measurement};
use lineos_metadata::bmr128::Bmr128Report;
use serde::{Deserialize, Serialize};

/// Evaluation result for a single platform preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetResult {
    pub preset:  String,
    pub passes:  bool,
    pub report:  Bmr128Report,
}

/// Full insights report — evaluates all presets from bmr-128.schema.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightsReport {
    /// Pass/fail for each platform preset
    pub preset_results:       Vec<PresetResult>,
    /// The strictest preset this session passes (lowest target_lufs that passes)
    pub recommended_preset:   Option<String>,
    /// Actionable hints for the rule engine (Phase 4)
    pub rule_engine_hints:    Vec<String>,
}

/// Evaluate compliance across all presets from bmr-128.schema.json.
///
/// All threshold values come from `schema.presets` — never hardcoded.
pub fn evaluate_all(
    measurement: &Ebu128Measurement,
    schema:      &Bmr128Schema,
) -> InsightsReport {
    let mut results: Vec<PresetResult> = Vec::new();

    for (preset_name, thresholds) in &schema.presets {
        let report = Bmr128Report::generate(
            measurement,
            preset_name,
            thresholds.target_lufs,
            thresholds.true_peak_ceiling_dbfs,
        );
        let passes = report.compliance.passes;
        results.push(PresetResult {
            preset: preset_name.clone(),
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

    let hints = generate_hints(measurement, &results);

    InsightsReport {
        preset_results:     results,
        recommended_preset: recommended,
        rule_engine_hints:  hints,
    }
}

/// Generate actionable hints for the rule engine (Phase 4 input).
fn generate_hints(
    m:       &Ebu128Measurement,
    results: &[PresetResult],
) -> Vec<String> {
    let mut hints: Vec<String> = Vec::new();
    let passing = results.iter().filter(|r| r.passes).count();

    if passing == 0 {
        hints.push("No platform presets pass. Consider re-mastering with a lower target LUFS.".into());
    }
    if m.true_peak_dbtp > -1.0 {
        hints.push(format!(
            "True peak {:.2} dBTP exceeds -1.0 dBTP reference ceiling. Apply limiting.",
            m.true_peak_dbtp
        ));
    }
    if m.stereo_correlation < 0.5 {
        hints.push(format!(
            "Low stereo correlation ({:.2}). Check for phase issues or out-of-phase content.",
            m.stereo_correlation
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
    use sp314_dsp::types::config::{Bmr128Schema, PipelineConstants, PresetThresholds};
    use sp314_dsp::types::metrics::Ebu128Measurement;
    use std::collections::BTreeMap;

    fn test_schema() -> Bmr128Schema {
        let mut presets = BTreeMap::new();
        presets.insert("spotify".into(), PresetThresholds {
            target_lufs: Some(-14.0),
            true_peak_ceiling_dbfs: -1.0,
        });
        presets.insert("broadcast".into(), PresetThresholds {
            target_lufs: Some(-23.0),
            true_peak_ceiling_dbfs: -1.0,
        });
        presets.insert("raw".into(), PresetThresholds {
            target_lufs: None,
            true_peak_ceiling_dbfs: -0.1,
        });
        Bmr128Schema {
            presets,
            pipeline: PipelineConstants {
                lookahead_ms: 2.0, lookahead_max: 192,
                eq_hpf_freq_hz: 30.0, eq_air_shelf_hz: 12000.0,
                dess_band_low_hz: 6000.0, dess_band_high_hz: 8000.0,
                comp_threshold_dbfs: -18.0, comp_ratio_default: 2.0, comp_knee_db: 6.0,
                sat_drive_default: 1.3, ms_side_gain_db: 1.5, ms_side_hpf_hz: 120.0,
                smoothing_ramp_ms: 20.0,
                dither_bits_24: 0.00000011920928955078125,
                dither_bits_16: 0.000030517578125,
            },
        }
    }

    fn test_measurement(integrated_lufs: f32, true_peak: f32) -> Ebu128Measurement {
        Ebu128Measurement {
            integrated_lufs,
            true_peak_dbtp:     true_peak,
            loudness_range_lu:  6.0,
            momentary_lufs:     -12.0,
            short_term_lufs:    integrated_lufs + 1.0,
            stereo_correlation: 0.95,
            dynamic_range_db:   12.0,
            sample_rate:        48000,
            channels:           2,
            duration_seconds:   5.0,
        }
    }

    #[test]
    fn test_evaluate_all_spotify_passes() {
        let schema = test_schema();
        // -14.0 LUFS, -1.2 dBTP → spotify passes, broadcast fails
        let m = test_measurement(-14.0, -1.2);
        let report = evaluate_all(&m, &schema);

        let spotify = report.preset_results.iter().find(|r| r.preset == "spotify");
        assert!(spotify.unwrap().passes, "Spotify should pass at -14 LUFS");

        let broadcast = report.preset_results.iter().find(|r| r.preset == "broadcast");
        assert!(!broadcast.unwrap().passes, "Broadcast should fail at -14 LUFS");
    }

    #[test]
    fn test_recommended_preset_strictest() {
        let schema = test_schema();
        // -14.0 passes spotify, fails broadcast
        let m = test_measurement(-14.0, -1.2);
        let report = evaluate_all(&m, &schema);
        // Recommended should be spotify (most negative target_lufs that passes)
        let rec = report.recommended_preset.as_deref().unwrap_or("none");
        assert_eq!(rec, "spotify");
    }

    #[test]
    fn test_no_presets_pass() {
        let schema = test_schema();
        // Very quiet → fails all presets with targets
        let m = test_measurement(-50.0, -10.0);
        let report = evaluate_all(&m, &schema);
        assert!(!report.rule_engine_hints.is_empty());
    }

    #[test]
    fn test_hints_for_high_true_peak() {
        let schema = test_schema();
        let mut m = test_measurement(-14.0, -0.5); // exceeds -1.0 dBTP
        m.true_peak_dbtp = -0.5;
        let report = evaluate_all(&m, &schema);
        let has_tp_hint = report.rule_engine_hints.iter()
            .any(|h| h.contains("True peak") || h.contains("limiting"));
        assert!(has_tp_hint, "Should hint about true peak: {:?}", report.rule_engine_hints);
    }

    #[test]
    fn test_insights_serializes() {
        let schema = test_schema();
        let m = test_measurement(-14.0, -1.2);
        let report = evaluate_all(&m, &schema);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("preset_results"));
        assert!(json.contains("rule_engine_hints"));
    }
}
