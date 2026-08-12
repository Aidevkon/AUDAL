//! One catalogue for preset strings.
//!
//! A preset id decided two unrelated things from two hand-kept copies of the
//! same list: which DSP path runs (m0-daemon's ContentType::from_preset) and
//! what the output must meet (LoudnessTarget::from_preset). The lists agreed,
//! but nothing made them agree — adding a preset meant remembering two files
//! in two crates.
//!
//! Three namespaces share that one string field today: content kind, delivery
//! target, and — per the comments in content_type.rs — flavour ids from
//! xaak/src/flavours.rs that must never arrive here at all. This table
//! separates the first two and lets `lookup` return None for the third, so
//! "unknown" becomes something a caller can see instead of a silent fall to
//! Music (F-024).

use crate::config::LoudnessTarget;

/// Which processing path the material takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Episode,
    Music,
}

/// Πώς δρομολογείται το render. ΔΕΝ είναι preset και
/// ΔΕΝ είναι flavour — είναι τρίτο είδος τιμής που
/// ταξίδευε στο ίδιο String.
///
/// ΗΤΑΝ: τρία string matches στο dsp_pipeline
/// (γρ.571, 865, 1082), η ίδια συνθήκη γραμμένη με
/// τρεις διαφορετικούς τρόπους. Ένα τυπογραφικό σε
/// οποιαδήποτε περνούσε σιωπηλά.
///
/// Το preset_id ΜΕΝΕΙ String στα structs, στο DB και
/// στο API. Αυτό που αλλάζει είναι ότι δεν αποφασίζει
/// πια routing — η απόφαση παίρνεται ΜΙΑ φορά και
/// ταξιδεύει ως τύπος.
///
/// ΤΟ "stereo_master" ΔΕΝ ΕΙΝΑΙ ΕΔΩ: δεν ελέγχεται
/// πουθενά στο src/. Εμφανίζεται μόνο σε δύο tests
/// που περνάνε string που κανείς δεν διαβάζει.
/// StereoOnly είναι η ΑΠΟΥΣΙΑ spatial, όχι ένα όνομα.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingMode {
    StereoOnly,
    SpatialUpmix,
    Both,
}

impl RoutingMode {
    /// ΜΙΑ πηγή αλήθειας για τη μετάφραση.
    /// Άγνωστο string → StereoOnly, γιατί η spatial
    /// έξοδος πρέπει να ζητηθεί ΡΗΤΑ.
    pub fn from_preset_id(preset_id: &str) -> Self {
        match preset_id {
            "spatial_upmix"   => Self::SpatialUpmix,
            "pro_bundle_both" => Self::Both,
            _                 => Self::StereoOnly,
        }
    }

    /// Χρειάζεται spatial έξοδος;
    pub fn needs_spatial(&self) -> bool {
        matches!(self, Self::SpatialUpmix | Self::Both)
    }
}

/// What the destination requires of the finished file.
/// Borrowed platform name rather than String, so the catalogue can be static.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeliverySpec {
    pub platform: &'static str,
    pub target_lufs: f32,
    pub max_true_peak_db: f32,
    pub max_lra_lu: Option<f32>,
    /// ACX specifies loudness as an RMS window, not a LUFS target — the only
    /// spec here that does. None means the platform has no RMS opinion, which
    /// is every platform but one. Verified against ACX's published submission
    /// requirements, 2026-07-29.
    pub rms_window_db: Option<(f32, f32)>,
    /// Maximum permitted noise floor, dBFS RMS. Also ACX-only today. This is
    /// the requirement that actually rejects files: gain moves the signal and
    /// the floor together, so a recording whose SNR is too low cannot be
    /// rescued by mastering at all.
    pub max_noise_floor_db: Option<f32>,
}

impl From<DeliverySpec> for LoudnessTarget {
    fn from(d: DeliverySpec) -> Self {
        Self {
            target_lufs: d.target_lufs,
            max_true_peak_db: d.max_true_peak_db,
            max_lra_lu: d.max_lra_lu,
            platform: d.platform.into(),
        }
    }
}

pub const SPOTIFY: DeliverySpec = DeliverySpec {
    platform: "spotify",
    target_lufs: -14.0,
    max_true_peak_db: -1.0,
    max_lra_lu: None,
    rms_window_db: None,
    max_noise_floor_db: None,
};

pub const YOUTUBE: DeliverySpec = DeliverySpec {
    platform: "youtube",
    target_lufs: -14.0,
    max_true_peak_db: -1.0,
    max_lra_lu: None,
    rms_window_db: None,
    max_noise_floor_db: None,
};

pub const BROADCAST: DeliverySpec = DeliverySpec {
    platform: "broadcast",
    target_lufs: -23.0,
    max_true_peak_db: -1.0,
    max_lra_lu: Some(20.0),
    rms_window_db: None,
    max_noise_floor_db: None,
};

pub const PODCAST: DeliverySpec = DeliverySpec {
    platform: "podcast",
    target_lufs: -16.0,
    max_true_peak_db: -1.0,
    max_lra_lu: None,
    rms_window_db: None,
    max_noise_floor_db: None,
};

/// ACX (Audiobook Creation Exchange — the submission path to Audible).
///
/// The only spec here that is not LUFS-based. Verified against ACX's published
/// submission requirements, 2026-07-29:
///   RMS between -23 and -18 dBFS, peak no higher than -3 dB,
///   noise floor no higher than -60 dBFS RMS.
///
/// target_lufs below is a STARTING PROXY, not a requirement — ACX states no
/// LUFS figure. -20.5 is the middle of the RMS window, used only to give the
/// existing LUFS-based gain stage something to aim at; the RMS window is what
/// decides pass or fail. The two measurements are not interchangeable: LUFS is
/// K-weighted and gates out silence, so a narration file sitting at -20.5 dB
/// RMS overall will not read -20.5 LUFS, and by how much depends on how much
/// of the file is pause. That offset needs measuring on real narration before
/// this number is trusted for anything.
///
/// max_true_peak_db is set to ACX's -3. ACX states it as a peak value and the
/// community reads it as sample peak; the engine enforces true peak, which is
/// stricter. Erring on the strict side is deliberate.
pub const ACX: DeliverySpec = DeliverySpec {
    platform: "acx",
    target_lufs: -20.5,
    max_true_peak_db: -3.0,
    max_lra_lu: None,
    rms_window_db: Some((-23.0, -18.0)),
    max_noise_floor_db: Some(-60.0),
};

pub struct PresetEntry {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    pub content: ContentKind,
    pub delivery: DeliverySpec,
}

/// The catalogue as it stands today. Deliberately a transcription of the two
/// existing match arms, not an improvement on them: "acx" and "apple_podcasts"
/// are aliases of "podcast" here because that is what they are in the code
/// being replaced. Giving ACX its own numbers is a later, separate step with
/// its own measurements.
pub static CATALOGUE: &[PresetEntry] = &[
    PresetEntry {
        id: "spotify",
        aliases: &["spotifyv3", "streaming"],
        content: ContentKind::Music,
        delivery: SPOTIFY,
    },
    PresetEntry {
        id: "youtube",
        aliases: &[],
        content: ContentKind::Music,
        delivery: YOUTUBE,
    },
    PresetEntry {
        id: "broadcast",
        aliases: &["broadcastvideo", "atscA85"],
        content: ContentKind::Music,
        delivery: BROADCAST,
    },
    PresetEntry {
        id: "podcast",
        aliases: &["spoken_word", "episode", "apple_podcasts"],
        content: ContentKind::Episode,
        delivery: PODCAST,
    },
    PresetEntry {
        id: "acx",
        // No "audiobook" alias. Audiobook is the content kind; ACX is one
        // destination for it, with its own numbers. Findaway, Kobo and Google
        // Play take audiobooks too and do not share this spec. Aliasing the
        // genre to one distributor is the same conflation this catalogue was
        // built to undo.
        aliases: &[],
        content: ContentKind::Episode,
        delivery: ACX,
    },
];

/// None means the string is not a preset — a flavour id, a typo, or something
/// from a namespace that should never have reached here. Callers decide what
/// to do about it; this function does not guess.
pub fn lookup(preset_id: &str) -> Option<&'static PresetEntry> {
    CATALOGUE
        .iter()
        .find(|e| e.id == preset_id || e.aliases.contains(&preset_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers themselves, written out. This test began as a parity check
    /// against the four hardcoded constructors it replaced; once those became
    /// delegates to these constants the comparison compared the table to
    /// itself. What is worth pinning now is the values, so a careless edit to
    /// the table shows up as a failing assert rather than as a quietly
    /// different master.
    #[test]
    fn catalogue_values_are_what_we_think_they_are() {
        for (id, lufs, peak, lra, platform) in [
            ("spotify", -14.0, -1.0, None, "spotify"),
            ("spotifyv3", -14.0, -1.0, None, "spotify"),
            ("streaming", -14.0, -1.0, None, "spotify"),
            ("youtube", -14.0, -1.0, None, "youtube"),
            ("broadcast", -23.0, -1.0, Some(20.0), "broadcast"),
            ("broadcastvideo", -23.0, -1.0, Some(20.0), "broadcast"),
            ("atscA85", -23.0, -1.0, Some(20.0), "broadcast"),
            ("podcast", -16.0, -1.0, None, "podcast"),
            ("spoken_word", -16.0, -1.0, None, "podcast"),
            ("episode", -16.0, -1.0, None, "podcast"),
            ("apple_podcasts", -16.0, -1.0, None, "podcast"),
        ] {
            let d = lookup(id).unwrap().delivery;
            assert_eq!(d.target_lufs, lufs, "{id} lufs");
            assert_eq!(d.max_true_peak_db, peak, "{id} peak");
            assert_eq!(d.max_lra_lu, lra, "{id} lra");
            assert_eq!(d.platform, platform, "{id} platform");
            assert!(d.rms_window_db.is_none(), "{id} should have no RMS window");
            assert!(
                d.max_noise_floor_db.is_none(),
                "{id} should have no floor limit"
            );
        }
    }

    /// ACX is the one spec that is not LUFS-based, so it gets its own test
    /// rather than a row in the table above. These three numbers are the whole
    /// requirement, verified against ACX's published submission requirements
    /// on 2026-07-29. If they change, a lot downstream changes with them.
    #[test]
    fn acx_carries_the_published_requirements() {
        let d = lookup("acx").unwrap().delivery;
        assert_eq!(d.rms_window_db, Some((-23.0, -18.0)), "RMS window");
        assert_eq!(d.max_true_peak_db, -3.0, "peak ceiling");
        assert_eq!(d.max_noise_floor_db, Some(-60.0), "noise floor");
        assert_eq!(d.platform, "acx");

        // The proxy target sits inside the window it stands in for. It is not
        // a requirement and the comment on ACX says why; this only checks it
        // has not drifted somewhere absurd.
        let (lo, hi) = d.rms_window_db.unwrap();
        assert!(
            d.target_lufs > lo && d.target_lufs < hi,
            "proxy target {} outside the RMS window it approximates",
            d.target_lufs
        );
    }

    /// acx used to be an alias of podcast, which meant an audiobook was
    /// mastered to -16 LUFS with a -1 dB ceiling and shipped to a spec that
    /// asks for neither. It is its own entry now.
    #[test]
    fn acx_is_not_podcast() {
        let acx = lookup("acx").unwrap();
        let pod = lookup("podcast").unwrap();
        assert_ne!(acx.delivery, pod.delivery);
        assert_eq!(acx.content, pod.content, "both are still speech");
    }

    /// The constructors are thin wrappers now; this proves they stayed thin.
    #[test]
    fn constructors_delegate_to_the_table() {
        assert_eq!(LoudnessTarget::spotify().target_lufs, SPOTIFY.target_lufs);
        assert_eq!(LoudnessTarget::youtube().target_lufs, YOUTUBE.target_lufs);
        assert_eq!(LoudnessTarget::broadcast().max_lra_lu, BROADCAST.max_lra_lu);
        assert_eq!(LoudnessTarget::podcast().platform, PODCAST.platform);
    }

    #[test]
    fn catalogue_matches_current_content_kinds() {
        for id in ["podcast", "spoken_word", "episode", "acx", "apple_podcasts"] {
            assert_eq!(lookup(id).unwrap().content, ContentKind::Episode, "{id}");
        }
        for id in [
            "spotify",
            "spotifyv3",
            "streaming",
            "youtube",
            "broadcast",
            "broadcastvideo",
            "atscA85",
        ] {
            assert_eq!(lookup(id).unwrap().content, ContentKind::Music, "{id}");
        }
    }

    /// The test in content_type.rs that wanted to assert this had to settle for
    /// checking the fallback value, admitting in its own comment that it could
    /// not tell "unknown" from "Music". With Option it can.
    #[test]
    fn flavour_ids_are_not_presets() {
        for f in [
            "neutral",
            "warm_analog",
            "club_punch",
            "radio_edit",
            "cinematic_wide",
            "clean_clear",
        ] {
            assert!(lookup(f).is_none(), "{f} must not resolve as a preset");
        }
    }

    #[test]
    fn unknown_is_none() {
        assert!(lookup("this_is_definitely_not_a_real_preset").is_none());
        assert!(lookup("").is_none());
    }

    /// The drift this table exists to prevent, now caught at test time.
    #[test]
    fn no_string_appears_twice() {
        let mut seen = std::vec::Vec::new();
        for e in CATALOGUE {
            for s in std::iter::once(&e.id).chain(e.aliases.iter()) {
                assert!(!seen.contains(s), "duplicate preset string: {s}");
                seen.push(s);
            }
        }
    }
}
