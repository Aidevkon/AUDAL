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
        // The list lives in lineos_types::presets::CATALOGUE now, shared with
        // LoudnessTarget::from_preset. Behaviour is unchanged: a recognised id
        // maps to its ContentKind, anything else warns and falls to Music.
        //
        // What changed is that the fallback can now say WHY. lookup() returning
        // None means the string is not a preset at all — a flavour id from
        // xaak/src/flavours.rs, an intent-knob value, or a typo. Before, an
        // unrecognised string and a genuine Music preset were indistinguishable
        // once this function returned (F-024).
        match lineos_types::presets::lookup(preset_id) {
            Some(entry) => match entry.content {
                lineos_types::presets::ContentKind::Episode => Self::Episode,
                lineos_types::presets::ContentKind::Music => Self::Music,
            },
            None => {
                tracing::warn!(
                    preset_id = %preset_id,
                    "preset_id is not in the catalogue — not a delivery target. \
                     Check whether a flavour_id or intent value was passed here \
                     by mistake. Defaulting to Music."
                );
                Self::Music
            }
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

#[cfg(test)]
mod content_type_tests {
    use super::*;

    #[test]
    fn known_episode_presets_map_correctly() {
        for preset in ["podcast", "spoken_word", "episode", "acx", "apple_podcasts"] {
            assert_eq!(
                ContentType::from_preset(preset),
                ContentType::Episode,
                "expected {preset} to map to Episode"
            );
        }
    }

    #[test]
    fn known_music_presets_map_correctly() {
        for preset in [
            "spotify",
            "spotifyv3",
            "streaming",
            "youtube",
            "broadcast",
            "broadcastvideo",
            "atscA85",
        ] {
            assert_eq!(
                ContentType::from_preset(preset),
                ContentType::Music,
                "expected {preset} to map to Music"
            );
        }
    }

    #[test]
    fn unknown_preset_still_defaults_to_music_but_is_now_logged() {
        // Behavior is unchanged (still defaults to Music, no signature
        // change, no call site disruption) — this test locks in that
        // the FALLBACK VALUE is identical to before. The actual
        // warn!() emission isn't unit-testable without a tracing
        // subscriber harness, which is out of scope for this small
        // fix; the log line itself was verified by eye in the diff.
        // This test's job is to prove the unknown case doesn't crash
        // and doesn't accidentally start returning something else.
        assert_eq!(
            ContentType::from_preset("this_is_definitely_not_a_real_preset"),
            ContentType::Music
        );
    }

    #[test]
    fn flavour_id_strings_correctly_fall_to_unknown_path_not_recognized() {
        // Deliberate: flavour strings are a DIFFERENT namespace and
        // must NOT be silently recognized here — if one reaches this
        // function, it's a real bug elsewhere that should surface via
        // the warn!() path (verified by eye), not be quietly absorbed
        // as if it were a legitimate unrecognized-but-fine preset.
        for flavour in ["warm_analog", "club_punch", "cinematic_wide"] {
            assert_eq!(
                ContentType::from_preset(flavour),
                ContentType::Music,
                "flavour strings still fall through to the Music \
                 default, but via the WARNING path, not a recognized \
                 preset match — confirmed by code inspection of the \
                 match arms, not distinguishable by return value alone"
            );
        }
    }
}
