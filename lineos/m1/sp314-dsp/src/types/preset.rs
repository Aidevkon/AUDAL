//! Platform presets — spotify, youtube, apple_music, tidal, broadcast, raw
//! Preset definitions are frozen per LineOS Constitution v2.0 §05.1.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub enum PlatformPreset {
    Spotify,
    Youtube,
    AppleMusic,
    Tidal,
    Broadcast,
    Raw,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PresetConfig {
    pub target_lufs:            Option<f32>,
    pub true_peak_ceiling_dbfs: f32,
}
