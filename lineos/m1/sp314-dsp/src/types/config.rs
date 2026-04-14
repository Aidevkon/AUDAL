//! Pipeline configuration — loaded from bmr-128.schema.json
//! Never hardcode these values. Always load from schema.
//! Authority: LineOS Constitution v2.0 §05 · LineOS §12 "BMR-128 thresholds hardcoded — build failure"

use alloc::collections::BTreeMap;
use alloc::string::String;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Bmr128Schema {
    pub presets:  BTreeMap<String, PresetThresholds>,
    pub pipeline: PipelineConstants,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PresetThresholds {
    pub target_lufs:             Option<f32>,
    pub true_peak_ceiling_dbfs:  f32,
}

/// All pipeline constants — sourced from bmr-128.schema.json at startup.
/// Never read these from hardcoded values.
#[derive(Debug, Clone, Deserialize)]
pub struct PipelineConstants {
    pub lookahead_ms:        f32,
    pub lookahead_max:       usize,
    pub eq_hpf_freq_hz:      f32,
    pub eq_air_shelf_hz:     f32,
    pub dess_band_low_hz:    f32,
    pub dess_band_high_hz:   f32,
    pub comp_threshold_dbfs: f32,
    pub comp_ratio_default:  f32,
    pub comp_knee_db:        f32,
    pub sat_drive_default:   f32,
    pub ms_side_gain_db:     f32,
    pub ms_side_hpf_hz:      f32,
    pub smoothing_ramp_ms:   f32,
    pub dither_bits_24:      f32,
    pub dither_bits_16:      f32,
}
