//! flavours.rs — Zero-Latency Flavour Presets.
//! Compile-time DspState constants — zero heap allocation.
//! Authority: lineos/docs/flavour-presets-spec-v1_0.md

use crate::repo::DspState;

pub const FLAVOUR_NEUTRAL: DspState = DspState {
    ducking_depth: 1.0,
    sidechain_hold: 3,
    ms_width: 1.0,
    lfe_gain: 0.0,
};

pub const FLAVOUR_WARM_ANALOG: DspState = DspState {
    ducking_depth: 1.0,
    sidechain_hold: 4,
    ms_width: 1.1,
    lfe_gain: 1.5,
};

pub const FLAVOUR_CLUB_PUNCH: DspState = DspState {
    ducking_depth: 1.5,
    sidechain_hold: 5,
    ms_width: 1.0,
    lfe_gain: 2.0,
};

pub const FLAVOUR_RADIO_EDIT: DspState = DspState {
    ducking_depth: 0.7,
    sidechain_hold: 2,
    ms_width: 0.85,
    lfe_gain: 0.0,
};

pub const FLAVOUR_CINEMATIC_WIDE: DspState = DspState {
    ducking_depth: 1.0,
    sidechain_hold: 6,
    ms_width: 1.5,
    lfe_gain: 1.0,
};

pub const FLAVOUR_CLEAN_CLEAR: DspState = DspState {
    ducking_depth: 0.9,
    sidechain_hold: 2,
    ms_width: 1.0,
    lfe_gain: -1.0,
};

/// All flavours — for iteration at AudioRepo init.
pub const ALL: &[(&str, DspState)] = &[
    ("neutral", FLAVOUR_NEUTRAL),
    ("warm_analog", FLAVOUR_WARM_ANALOG),
    ("club_punch", FLAVOUR_CLUB_PUNCH),
    ("radio_edit", FLAVOUR_RADIO_EDIT),
    ("cinematic_wide", FLAVOUR_CINEMATIC_WIDE),
    ("clean_clear", FLAVOUR_CLEAN_CLEAR),
];

/// Resolve flavour name → DspState.
/// Returns None for unknown names.
pub fn from_name(name: &str) -> Option<DspState> {
    ALL.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// Human-readable label for UI display.
pub fn label(name: &str) -> &'static str {
    match name {
        "neutral" => "Neutral",
        "warm_analog" => "Warm Analog",
        "club_punch" => "Club Punch",
        "radio_edit" => "Radio Edit",
        "cinematic_wide" => "Cinematic Wide",
        "clean_clear" => "Clean & Clear",
        _ => "Unknown",
    }
}
