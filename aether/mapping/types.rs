// aether/mapping/types.rs — MicroDelta types and constants
// Authority: spec/locked/S-005_macro_micro_mapping.md v1.0

// EQ bounds
pub const EQ_GAIN_DELTA_MIN_DB: f32 = -6.0;
pub const EQ_GAIN_DELTA_MAX_DB: f32 =  6.0;
pub const EQ_FREQ_DELTA_MIN_HZ: f32 = -500.0;
pub const EQ_FREQ_DELTA_MAX_HZ: f32 =  500.0;

// Dynamics bounds
pub const COMP_THRESHOLD_DELTA_MIN_DB: f32 = -12.0;
pub const COMP_THRESHOLD_DELTA_MAX_DB: f32 =   0.0;
pub const COMP_RATIO_DELTA_MIN:        f32 =  -3.0;
pub const COMP_RATIO_DELTA_MAX:        f32 =   3.0;
pub const COMP_ATTACK_DELTA_MIN_MS:    f32 = -20.0;
pub const COMP_ATTACK_DELTA_MAX_MS:    f32 =  20.0;
pub const COMP_RELEASE_DELTA_MIN_MS:   f32 = -80.0;
pub const COMP_RELEASE_DELTA_MAX_MS:   f32 =  80.0;

// Saturation bounds
pub const SAT_DRIVE_DELTA_MIN: f32 = -0.4;
pub const SAT_DRIVE_DELTA_MAX: f32 =  0.4;
pub const SAT_MIX_DELTA_MIN:   f32 = -0.3;
pub const SAT_MIX_DELTA_MAX:   f32 =  0.3;

// Stereo bounds
pub const STEREO_WIDTH_DELTA_MIN: f32 = -0.2;
pub const STEREO_WIDTH_DELTA_MAX: f32 =  0.2;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EqDelta {
    pub low_shelf_gain_db:  f32,
    pub low_shelf_freq_hz:  f32,
    pub high_shelf_gain_db: f32,
    pub high_shelf_freq_hz: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DynamicsDelta {
    pub comp_threshold_db: f32,
    pub comp_ratio:        f32,
    pub comp_attack_ms:    f32,
    pub comp_release_ms:   f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SaturationDelta {
    pub drive: f32,
    pub mix:   f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StereoDelta {
    /// Always 0.0 from macro mapping.
    /// Stereo width is set by S-006 (Chaos Engine).
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MicroDelta {
    pub eq:      EqDelta,
    pub dynamics: DynamicsDelta,
    pub sat:     SaturationDelta,
    pub stereo:  StereoDelta,
}

impl MicroDelta {
    pub fn zero() -> Self {
        Self {
            eq: EqDelta {
                low_shelf_gain_db:  0.0,
                low_shelf_freq_hz:  0.0,
                high_shelf_gain_db: 0.0,
                high_shelf_freq_hz: 0.0,
            },
            dynamics: DynamicsDelta {
                comp_threshold_db: 0.0,
                comp_ratio:        0.0,
                comp_attack_ms:    0.0,
                comp_release_ms:   0.0,
            },
            sat: SaturationDelta { drive: 0.0, mix: 0.0 },
            stereo: StereoDelta { width: 0.0 },
        }
    }
}

impl Default for MicroDelta {
    fn default() -> Self { Self::zero() }
}
