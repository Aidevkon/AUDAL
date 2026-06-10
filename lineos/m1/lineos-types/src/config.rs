// src/config.rs
// Replaces: sp314_dsp::types::config::{Bmr128Schema, PipelineConstants, PresetThresholds}

use serde::{Deserialize, Serialize};

/// Broadcast loudness compliance schema.
/// Replaces v2.9 Bmr128Schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoudnessTarget {
    pub target_lufs: f32,
    pub max_true_peak_db: f32,
    pub max_lra_lu: Option<f32>,
    pub platform: String,
}

/// Well-known platform targets.
impl LoudnessTarget {
    pub fn spotify() -> Self {
        Self {
            target_lufs: -14.0,
            max_true_peak_db: -1.0,
            max_lra_lu: None,
            platform: "spotify".into(),
        }
    }
    pub fn youtube() -> Self {
        Self {
            target_lufs: -14.0,
            max_true_peak_db: -1.0,
            max_lra_lu: None,
            platform: "youtube".into(),
        }
    }
    pub fn broadcast() -> Self {
        Self {
            target_lufs: -23.0,
            max_true_peak_db: -1.0,
            max_lra_lu: Some(20.0),
            platform: "broadcast".into(),
        }
    }
    pub fn podcast() -> Self {
        Self {
            target_lufs: -16.0,
            max_true_peak_db: -1.0,
            max_lra_lu: None,
            platform: "podcast".into(),
        }
    }
}

/// v2.9 compatibility aliases
pub type Bmr128Schema = LoudnessTarget;
pub type PresetThresholds = LoudnessTarget;

/// Pipeline processing constants.
/// Replaces v2.9 PipelineConstants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConstants {
    pub sample_rate: u32,
    pub block_size: usize,
    pub limiter_ceiling_db: f32,
}

impl Default for PipelineConstants {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            block_size: 512,
            limiter_ceiling_db: -0.5,
        }
    }
}
