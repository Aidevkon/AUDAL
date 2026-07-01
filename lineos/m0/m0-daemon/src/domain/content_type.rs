use crate::blob_store::StemFingerprints;
use sp314_dsp::stft::two_pass::{BandSpatialMetrics, RenderMetadata};

/// Content classification derived from
/// preset_id at pipeline entry.
/// Determines which DSP nodes run and
/// which are bypassed.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentType {
    /// Spoken-word: podcast, interview,
    /// field recording. Skips stem
    /// separation and spatial widening.
    /// Target: -16 LUFS.
    Episode,
    /// Music: all DSP nodes active.
    /// Target: -14 LUFS.
    Music,
}

impl ContentType {
    pub fn from_preset(preset_id: &str) -> Self {
        match preset_id {
            "podcast" | "spoken_word" | "episode" | "acx" | "apple_podcasts" => Self::Episode,
            _ => Self::Music,
        }
    }

    pub fn lufs_target(&self) -> f32 {
        match self {
            Self::Episode => -16.0,
            Self::Music => -14.0,
        }
    }

    pub fn skip_stems(&self) -> bool {
        matches!(self, Self::Episode)
    }

    pub fn skip_widening(&self) -> bool {
        matches!(self, Self::Episode)
    }

    /// Bypassed stem result for Episode
    /// mode — zero fingerprints, zero
    /// spatial metadata. Certificate
    /// will omit Stem DNA fields.
    pub fn bypassed_render() -> (StemFingerprints, RenderMetadata) {
        (
            StemFingerprints::default(),
            RenderMetadata {
                frames_written: 0,
                voice_transient_density: 0.0,
                drums_transient_density: 0.0,
                spatial: [BandSpatialMetrics::default(); 5],
            },
        )
    }
}
