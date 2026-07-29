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
        crate::presets::SPOTIFY.into()
    }
    pub fn youtube() -> Self {
        crate::presets::YOUTUBE.into()
    }
    pub fn broadcast() -> Self {
        crate::presets::BROADCAST.into()
    }
    pub fn podcast() -> Self {
        crate::presets::PODCAST.into()
    }

    /// Resolve a preset_id string to its LoudnessTarget. Mirrors
    /// the existing ContentType::from_preset string-matching
    /// pattern — this mapping never existed for LoudnessTarget
    /// before (confirmed by recon 2026-07-17: neither v2 nor v3
    /// ever called it, both hardcoded -14.0 directly in their HTTP
    /// handlers instead). Unknown preset_ids fall back to
    /// spotify()'s -14.0 default rather than erroring — matches
    /// the existing unwrap_or(-14.0) fallback behavior callers
    /// already relied on, so this is a strict improvement, not a
    /// behavior change for the unknown-preset case.
    pub fn from_preset(preset_id: &str) -> Self {
        // The list this used to carry inline now lives in presets::CATALOGUE,
        // shared with m0-daemon's ContentType::from_preset. The spotify()
        // fallback for unknown ids is unchanged; callers that need to tell
        // "unknown" from "spotify" should call presets::lookup directly and
        // decide for themselves.
        match crate::presets::lookup(preset_id) {
            Some(entry) => entry.delivery.into(),
            None => Self::spotify(),
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
