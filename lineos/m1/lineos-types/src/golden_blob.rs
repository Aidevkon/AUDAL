// src/golden_blob.rs
// Replaces: sp314_dsp::types::golden_blob::{GoldenBlob, GoldenInputProfile, BlobType}

use crate::metrics::LufsReport;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlobType {
    Master,
    Preview,
    Stem,
    Reference,
    Audio,
}

/// Input profile — acoustic fingerprint of the source audio.
/// Replaces v2.9 GoldenInputProfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenInputProfile {
    pub integrated_lufs: f32,
    pub true_peak_dbfs: f32,
    pub crest_factor_db: f32,
    pub spectral_centroid: f32,
    pub dynamic_range_lu: f32,
    pub stereo_correlation: f32,
}

/// Final mastered output blob.
/// Replaces v2.9 GoldenBlob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenBlob {
    pub blob_type: BlobType,
    pub input_profile: GoldenInputProfile,
    pub output_lufs: LufsReport,
    pub sha256: String,
    pub preset_name: String,
    pub engine_version: String,
    #[serde(default = "default_schema_v1")]
    pub schema_version: u32,
    #[serde(default)]
    pub aether_cert: Option<String>,
    #[serde(default)]
    pub aether_persona: Option<String>,
    #[serde(default)]
    pub aether_config: Option<String>,
}

fn default_schema_v1() -> u32 {
    1
}
