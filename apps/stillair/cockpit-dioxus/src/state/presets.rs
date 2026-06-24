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
        label: "Spotify / Apple",
        lufs: -14.0,
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
];
