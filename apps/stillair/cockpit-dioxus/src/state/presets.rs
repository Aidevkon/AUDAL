pub struct FlavourDef {
    pub id: &'static str,
    pub label: &'static str,
}

pub struct PlatformDef {
    pub id: &'static str,
    pub label: &'static str,
    pub lufs: f32,
}

pub const FLAVOURS: &[FlavourDef] = &[
    FlavourDef {
        id: "clean",
        label: "Clean & Clear",
    },
    FlavourDef {
        id: "warm",
        label: "Warm Analog",
    },
    FlavourDef {
        id: "punch",
        label: "Club Punch",
    },
    FlavourDef {
        id: "air",
        label: "Open Air",
    },
    FlavourDef {
        id: "film",
        label: "Cinematic",
    },
    FlavourDef {
        id: "broadcast",
        label: "Radio Edit",
    },
];

pub const PLATFORMS: &[PlatformDef] = &[
    PlatformDef {
        id: "spotify",
        label: "Spotify",
        lufs: -14.0,
    },
    PlatformDef {
        id: "apple_music",
        label: "Apple Digital Masters",
        lufs: -16.0,
    },
    PlatformDef {
        id: "apple_podcast",
        label: "Podcast",
        lufs: -16.0,
    },
    PlatformDef {
        id: "broadcast",
        label: "Broadcast",
        lufs: -23.0,
    },
    PlatformDef {
        id: "youtube",
        label: "YouTube",
        lufs: -14.0,
    },
    // 2026-08-25: ΤΟ ΠΡΩΤΟ ΣΗΜΕΙΟ ΟΠΟΥ ΤΟ ACX ΓΙΝΕΤΑΙ ΕΠΙΛΕΞΙΜΟ.
    // Πριν από αυτή τη γραμμή, το `acx` υπήρχε στο lineos-types
    // CATALOGUE (presets.rs:200, δικά του νούμερα: -20.5 LUFS,
    // -3.0 dBTP, RMS window, noise floor) αλλά ΚΑΝΕΝΑ μενού δεν το
    // πρόσφερε — άρα `blob.core.preset_id == "acx"` δεν μπορούσε ποτέ
    // να συμβεί και το προϊόν δεν παρήγαγε αρχείο ACX καθόλου.
    // Το `lufs` εδώ είναι ΜΟΝΟ ετικέτα οθόνης· η αυθεντία της τιμής
    // είναι το CATALOGUE, μέσω LoudnessTarget::from_preset.
    PlatformDef {
        id: "acx",
        label: "ACX / Audiobook",
        lufs: -20.5,
    },
];
