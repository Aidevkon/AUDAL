// src/mastering.rs
// Replaces: sp314_dsp::pipeline::{MasteringPipeline, MasteringIntent}
// In v3, MasteringPipeline is replaced by DspGraph + pipelineforge.
// MasteringIntent is the high-level request — maps to EngineConfig.

use serde::{Deserialize, Serialize};
use crate::config::LoudnessTarget;

/// High-level mastering request.
/// Replaces v2.9 MasteringIntent.
/// In v3, this is translated to EngineConfig by the Rule Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteringIntent {
    pub target:      LoudnessTarget,
    pub preset_name: String,
    pub stem_mode:   bool,
    pub target_makeup_db: f32,
}

impl MasteringIntent {
    pub fn spotify() -> Self {
        Self {
            target:      LoudnessTarget::spotify(),
            preset_name: "SpotifyV3".into(),
            stem_mode:   false,
            target_makeup_db: 0.0,
        }
    }
    pub fn podcast() -> Self {
        Self {
            target:      LoudnessTarget::podcast(),
            preset_name: "Podcast".into(),
            stem_mode:   false,
            target_makeup_db: 0.0,
        }
    }
}

/// v2.9 MasteringPipeline is replaced by DspGraph in v3.
/// This stub exists only for migration — do not use for new code.
/// TODO: remove after all callers migrate to pipelineforge + openclaw.
#[deprecated(note = "Use pipelineforge::Pipelineforge + openclaw::OpenClawEngine instead")]
pub struct MasteringPipeline;

#[allow(deprecated)]
impl MasteringPipeline {
    pub fn new(_constants: crate::config::PipelineConstants) -> Self {
        Self
    }

    pub fn master(
        &mut self,
        _intent: &MasteringIntent,
        _chunks: &[crate::audio::AudioChunk],
        _seed: [u8; 32],
    ) -> Result<crate::golden_blob::GoldenBlob, String> {
        // Stub implementation
        Err("MasteringPipeline::master() is deprecated and not implemented in lineos-types".into())
    }
}
