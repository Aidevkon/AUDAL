use crate::blob_store::StemFingerprints;
use sp314_dsp::stft::two_pass::{BandSpatialMetrics, RenderMetadata};

pub use aether_bridge::ContentType;

/// Content classification derived from
/// preset_id at pipeline entry.
/// Determines which DSP nodes run and
/// which are bypassed.
///
/// Episode: Spoken-word: podcast, interview, field recording. Skips stem separation and spatial widening. Target: -16 LUFS.
/// Music: all DSP nodes active. Target: -14 LUFS.
pub trait ContentTypeExt {
    fn from_preset(preset_id: &str) -> Self;
    fn lufs_target(&self) -> f32;
    fn skip_stems(&self) -> bool;
    fn skip_widening(&self) -> bool;

    /// Bypassed stem result for Episode
    /// mode — zero fingerprints, zero
    /// spatial metadata. Certificate
    /// will omit Stem DNA fields.
    fn bypassed_render() -> (StemFingerprints, RenderMetadata);
}

impl ContentTypeExt for ContentType {
    fn from_preset(preset_id: &str) -> Self {
        match preset_id {
            "podcast" | "spoken_word" | "episode" | "acx" | "apple_podcasts" => Self::Episode,
            _ => Self::Music,
        }
    }

    fn lufs_target(&self) -> f32 {
        match self {
            Self::Episode => -16.0,
            Self::Music => -14.0,
        }
    }

    fn skip_stems(&self) -> bool {
        matches!(self, Self::Episode)
    }

    fn skip_widening(&self) -> bool {
        matches!(self, Self::Episode)
    }

    fn bypassed_render() -> (StemFingerprints, RenderMetadata) {
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
