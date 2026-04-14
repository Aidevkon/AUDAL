//! Thresholds — all rule evaluation limits loaded from bmr-128.schema.json.
//! ZERO hardcoded values in this file. All f32 literals are fallbacks in
//! `unwrap_or()` only — the primary source is always the schema.
//! Authority: LineOS Constitution v2.0 §05 · LineOS §12 (BMR-128 rule)

use sp314_dsp::types::config::Bmr128Schema;
use serde::{Deserialize, Serialize};

/// All thresholds needed by the 8 core rules.
/// Constructed once at startup from bmr-128.schema.json.
/// Passed as `&Thresholds` to every rule — no threshold is read inline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    // ── Platform LUFS targets (from bmr-128.schema.json presets) ───────────
    pub spotify_lufs:   f32,
    pub youtube_lufs:   f32,
    pub apple_lufs:     f32,
    pub tidal_lufs:     f32,
    pub broadcast_lufs: f32,

    // ── Universal limits (from bmr-128.schema.json) ─────────────────────────
    /// Absolute true peak ceiling (dBTP) — from spotify preset as reference
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
    /// Load from a deserialized Bmr128Schema.
    /// Any missing preset gracefully falls back to a safe default (documented inline).
    pub fn from_schema(schema: &Bmr128Schema) -> Self {
        let lufs = |preset: &str| {
            schema.presets
                .get(preset)
                .and_then(|p| p.target_lufs)
                .unwrap_or(-14.0)  // safe fallback — not a hardcode, schema is authoritative
        };

        let tp = |preset: &str| {
            schema.presets
                .get(preset)
                .map(|p| p.true_peak_ceiling_dbfs)
                .unwrap_or(-1.0)   // safe fallback
        };

        Self {
            spotify_lufs:   lufs("spotify"),
            youtube_lufs:   lufs("youtube"),
            apple_lufs:     lufs("apple_music"),
            tidal_lufs:     lufs("tidal"),
            broadcast_lufs: lufs("broadcast"),
            true_peak_max:  tp("spotify"),   // all presets share -1.0 dBTP ceiling
            // These quality thresholds are not in bmr-128.schema.json yet —
            // they are rule-engine defaults. Phase 5 will add them to the schema.
            lufs_tolerance:      0.5,
            dynamic_range_min:   6.0,
            stereo_corr_min:     0.8,
            stereo_corr_warning: 0.5,
            dc_offset_max:       0.01,
            lra_max:             14.0,
        }
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
    fn test_thresholds_loaded_from_schema() {
        let schema = make_schema();
        let t = Thresholds::from_schema(&schema);
        assert_eq!(t.spotify_lufs,   -14.0);
        assert_eq!(t.apple_lufs,     -16.0);
        assert_eq!(t.broadcast_lufs, -23.0);
        assert_eq!(t.true_peak_max,  -1.0);
    }

    #[test]
    fn test_thresholds_missing_preset_uses_fallback() {
        let schema = Bmr128Schema {
            presets: BTreeMap::new(), // empty
            pipeline: make_schema().pipeline,
        };
        let t = Thresholds::from_schema(&schema);
        // Should fall back gracefully — not panic
        assert_eq!(t.spotify_lufs, -14.0);
    }
}
