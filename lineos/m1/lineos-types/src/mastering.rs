// src/mastering.rs
// Replaces: sp314_dsp::pipeline::{MasteringPipeline, MasteringIntent}
// In v3, MasteringPipeline is replaced by DspGraph + pipelineforge.
// MasteringIntent is the high-level request — maps to EngineConfig.

use crate::config::LoudnessTarget;
use serde::{Deserialize, Serialize};

/// High-level mastering request.
/// Replaces v2.9 MasteringIntent.
/// In v3, this is translated to EngineConfig by the Rule Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteringIntent {
    pub target: LoudnessTarget,
    pub preset_name: String,
    pub stem_mode: bool,
    pub target_makeup_db: f32,
    /// Pre-computed blend release in ms.
    /// Computed by Control Plane (dsp_node)
    /// from intent_dynamics (0.0..1.0).
    /// DSP reads this directly — no math.
    /// Default: 95ms (neutral, intent=0.5)
    pub limiter_blend_release_ms: f32,
    /// Max limiter gain reduction allowed (dB).
    /// If projected peak > ceiling + this value,
    /// LUFS makeup is capped to preserve transients.
    /// Computed by Control Plane. DSP reads blindly.
    /// Default: 6.0dB (balanced headroom)
    pub max_limiter_gr_db: f32,
}

impl MasteringIntent {
    pub fn spotify() -> Self {
        Self {
            target: LoudnessTarget::spotify(),
            preset_name: "SpotifyV3".into(),
            stem_mode: false,
            target_makeup_db: 0.0,
            limiter_blend_release_ms: 95.0,
            max_limiter_gr_db: 6.0,
        }
    }
    pub fn podcast() -> Self {
        Self {
            target: LoudnessTarget::podcast(),
            preset_name: "Podcast".into(),
            stem_mode: false,
            target_makeup_db: 0.0,
            limiter_blend_release_ms: 95.0,
            max_limiter_gr_db: 6.0,
        }
    }
}
