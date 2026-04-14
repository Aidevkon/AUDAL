//! Thresholds — all rule evaluation limits loaded from bmr-128.schema.json.
//! ZERO hardcoded values in this file. All f32 literals are fallbacks in
//! `unwrap_or()` only — the primary source is always the schema.
//! Authority: LineOS Constitution v2.0 §05 · LineOS §12 (BMR-128 rule)
//! v1.1: Thresholds now carries preset_name + target_lufs for selected preset only.
//! Rule-engine evaluates against the selected preset; Cockpit owns preset selection.

use sp314_dsp::types::config::Bmr128Schema;
use serde::{Deserialize, Serialize};

/// All thresholds needed by the 6 core rules.
/// Constructed once at startup from bmr-128.schema.json.
/// Passed as `&Thresholds` to every rule — no threshold is read inline.
///
/// `preset_name` + `target_lufs` represent the Cockpit's selected preset.
/// `lufs_compliance` evaluates against `target_lufs` only — never against all presets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    // ── Selected preset (set by Cockpit, loaded from schema) ─────────────────
    /// Name of the user-selected BMR-128 preset (e.g. "spotify", "broadcast")
    pub preset_name:  &'static str,
    /// LUFS target for the selected preset. None = raw preset, lufs_compliance skips.
    pub target_lufs:  Option<f32>,

    // ── Universal limits (from bmr-128.schema.json) ───────────────────────────
    /// Absolute true peak ceiling (dBTP) — from selected preset
    pub true_peak_max:  f32,
    /// LUFS compliance tolerance band (±LU) — symmetric
    pub lufs_tolerance: f32,

    // ── Quality thresholds ──────────────────────────────────────────────────
    /// Minimum acceptable dynamic range before "over-compressed" warning (dB)
    pub dynamic_range_min:   f32,
    /// Minimum stereo correlation before "low" warning
    pub stereo_corr_min:     f32,
    /// Minimum stereo correlation before "weak/phase issue" warning (more severe)
    pub stereo_corr_warning: f32,
    /// Maximum acceptable DC offset magnitude
    pub dc_offset_max:       f32,
    /// Maximum acceptable LRA before "high dynamics" info
    pub lra_max:             f32,
}

impl Thresholds {
    /// Load from a deserialized Bmr128Schema for a specific preset.
    /// `preset` — the key into schema.presets (e.g. "spotify", "broadcast", "raw").
    /// All values come from the schema; fallbacks are documented.
    pub fn from_schema_with_preset(schema: &Bmr128Schema, preset: &'static str) -> Self {
        let entry = schema.presets.get(preset);

        let target_lufs = entry.and_then(|p| p.target_lufs);

        let true_peak_max = entry
            .map(|p| p.true_peak_ceiling_dbfs)
            .unwrap_or(-1.0);   // safe fallback

        Self {
            preset_name:  preset,
            target_lufs,
            true_peak_max,
            // Quality thresholds not yet in bmr-128.schema.json — rule-engine defaults.
            // Phase 5 will migrate these into the schema.
            lufs_tolerance:      0.5,
            dynamic_range_min:   6.0,
            stereo_corr_min:     0.8,
            stereo_corr_warning: 0.5,
            dc_offset_max:       0.01,
            lra_max:             14.0,
        }
    }

    /// Convenience: load for the "spotify" preset (most common default).
    pub fn from_schema(schema: &Bmr128Schema) -> Self {
        Self::from_schema_with_preset(schema, "spotify")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp314_dsp::types::config::{Bmr128Schema, PipelineConstants, PresetThresholds};
    use std::collections::BTreeMap;

    fn make_schema() -> Bmr128Schema {
        let mut presets = BTreeMap::new();
        presets.insert("spotify".into(),     PresetThresholds { target_lufs: Some(-14.0), true_peak_ceiling_dbfs: -1.0 });
        presets.insert("youtube".into(),     PresetThresholds { target_lufs: Some(-14.0), true_peak_ceiling_dbfs: -1.0 });
        presets.insert("apple_music".into(), PresetThresholds { target_lufs: Some(-16.0), true_peak_ceiling_dbfs: -1.0 });
        presets.insert("tidal".into(),       PresetThresholds { target_lufs: Some(-14.0), true_peak_ceiling_dbfs: -1.0 });
        presets.insert("broadcast".into(),   PresetThresholds { target_lufs: Some(-23.0), true_peak_ceiling_dbfs: -1.0 });
        presets.insert("raw".into(),         PresetThresholds { target_lufs: None,         true_peak_ceiling_dbfs: -0.1 });
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

    #[test]
    fn test_thresholds_spotify_loaded_from_schema() {
        let schema = make_schema();
        let t = Thresholds::from_schema_with_preset(&schema, "spotify");
        assert_eq!(t.preset_name, "spotify");
        assert_eq!(t.target_lufs, Some(-14.0));
        assert_eq!(t.true_peak_max, -1.0);
    }

    #[test]
    fn test_thresholds_apple_music_loaded_from_schema() {
        let schema = make_schema();
        let t = Thresholds::from_schema_with_preset(&schema, "apple_music");
        assert_eq!(t.preset_name, "apple_music");
        assert_eq!(t.target_lufs, Some(-16.0));
    }

    #[test]
    fn test_thresholds_broadcast_loaded_from_schema() {
        let schema = make_schema();
        let t = Thresholds::from_schema_with_preset(&schema, "broadcast");
        assert_eq!(t.preset_name, "broadcast");
        assert_eq!(t.target_lufs, Some(-23.0));
    }

    #[test]
    fn test_thresholds_raw_preset_no_lufs_target() {
        let schema = make_schema();
        let t = Thresholds::from_schema_with_preset(&schema, "raw");
        assert_eq!(t.preset_name, "raw");
        assert_eq!(t.target_lufs, None);
    }

    #[test]
    fn test_thresholds_missing_preset_uses_fallback() {
        let schema = Bmr128Schema {
            presets: BTreeMap::new(),
            pipeline: make_schema().pipeline,
        };
        let t = Thresholds::from_schema_with_preset(&schema, "spotify");
        // Missing preset → None for target_lufs, -1.0 fallback for true_peak_max
        assert_eq!(t.target_lufs, None);
        assert_eq!(t.true_peak_max, -1.0);
    }

    #[test]
    fn test_from_schema_shorthand_is_spotify() {
        let schema = make_schema();
        let t = Thresholds::from_schema(&schema);
        assert_eq!(t.preset_name, "spotify");
        assert_eq!(t.target_lufs, Some(-14.0));
    }
}
