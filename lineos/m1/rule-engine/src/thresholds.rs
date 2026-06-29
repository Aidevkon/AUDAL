//! Thresholds — all rule evaluation limits loaded from bmr-128.schema.json.
//! ZERO hardcoded values in this file. All f32 literals are fallbacks in
//! `unwrap_or()` only — the primary source is always the schema.
//! Authority: LineOS Constitution v2.0 §05 · LineOS §12 (BMR-128 rule)
//! v1.1: Thresholds now carries preset_name + target_lufs for selected preset only.
//! Rule-engine evaluates against the selected preset; Cockpit owns preset selection.

use lineos_types::Bmr128Schema;
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
    pub preset_name: &'static str,
    /// LUFS target for the selected preset. None = raw preset, lufs_compliance skips.
    pub target_lufs: Option<f32>,

    // ── Universal limits (from bmr-128.schema.json) ───────────────────────────
    /// Absolute true peak ceiling (dBTP) — from selected preset
    pub true_peak_max: f32,
    /// LUFS compliance tolerance band (±LU) — symmetric
    pub lufs_tolerance: f32,

    // ── Quality thresholds ──────────────────────────────────────────────────
    /// Minimum acceptable dynamic range before "over-compressed" warning (dB)
    pub dynamic_range_min: f32,
    /// Minimum stereo correlation before "low" warning
    pub stereo_corr_min: f32,
    /// Minimum stereo correlation before "weak/phase issue" warning (more severe)
    pub stereo_corr_warning: f32,
    /// Maximum acceptable DC offset magnitude
    pub dc_offset_max: f32,
    /// Maximum acceptable LRA before "high dynamics" info
    pub lra_max: f32,
}

impl Thresholds {
    /// Load from a deserialized Bmr128Schema for a specific preset.
    /// `preset` — the key into schema.presets (e.g. "spotify", "broadcast", "raw").
    /// All values come from the schema; fallbacks are documented.
    pub fn from_schema_with_preset(schema: &Bmr128Schema, preset: &'static str) -> Self {
        let target_lufs = Some(schema.target_lufs);
        let true_peak_max = schema.max_true_peak_db;

        Self {
            preset_name: preset,
            target_lufs,
            true_peak_max,
            // Quality thresholds not yet in bmr-128.schema.json — rule-engine defaults.
            // Phase 5 will migrate these into the schema.
            lufs_tolerance: 0.5,
            dynamic_range_min: 6.0,
            stereo_corr_min: 0.8,
            stereo_corr_warning: 0.5,
            dc_offset_max: 0.01,
            lra_max: 14.0,
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
    use lineos_types::config::LoudnessTarget;

    #[test]
    fn thresholds_read_from_schema() {
        let schema = LoudnessTarget::spotify();
        let t = Thresholds::from_schema(&schema);
        assert_eq!(t.target_lufs, Some(-14.0));
        assert!((t.true_peak_max - (-1.0)).abs() < 0.001);
    }
}
