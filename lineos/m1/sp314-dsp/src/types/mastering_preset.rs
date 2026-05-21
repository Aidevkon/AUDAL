//! MasteringPreset + platform presets — sp314-dsp v2.9 §Preset System.
//! dither_seed values are stable compile-time constants (major version bump if changed).

/// Compressor personality; drives stereo link and attack/release tables (§N5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionStyle {
    Transparent,
    Gentle,
    Medium,
    Aggressive,
}

impl CompressionStyle {
    /// Stereo link amount — compile-time constant per style (v2.9).
    pub const fn stereo_link_amount(&self) -> f32 {
        match self {
            Self::Transparent => 0.3,
            Self::Gentle      => 0.5,
            Self::Medium      => 0.7,
            Self::Aggressive  => 1.0,
        }
    }

    /// Attack/release coefficient: α = 1 - exp(-1 / (time_ms × 0.001 × sample_rate)) (v2.9.1 §N5).
    pub fn time_to_coeff(time_ms: f32, sample_rate: f32) -> f32 {
        if time_ms <= 0.0 {
            return 1.0;
        }
        let time_samples = time_ms * 0.001 * sample_rate;
        1.0 - libm::expf(-1.0 / time_samples)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaturationStyle {
    None,
    Tape,
    Tube,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqCurve {
    Flat,
    Air,
    Warmth,
    Presence,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimiterAlgorithm {
    Transparent,
    Punchy,
    Brickwall,
}

/// Full mastering preset validated at pipeline entry (`validate_preset`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MasteringPreset {
    pub lufs_target:    f32,
    pub true_peak_ceil: f32,
    pub compression:    CompressionStyle,
    pub saturation:     SaturationStyle,
    pub eq_curve:       EqCurve,
    pub limiter:        LimiterAlgorithm,
    pub gain_budget_db: f32,
    pub dither_bits:    u8,
    pub dither_seed:    u64,
    pub oversampling:   u8,
}

/// Default cross-stage gain budget (dB).
const DEFAULT_GAIN_BUDGET_DB: f32 = 6.0;

// ── Stable dither seeds (spec table — do not change without major version bump) ──
const DITHER_SEED_SPOTIFY: u64        = 0x5A314_5350;
const DITHER_SEED_YOUTUBE: u64        = 0x5A314_5951;
const DITHER_SEED_APPLE_MUSIC: u64    = 0x5A314_414D;
const DITHER_SEED_APPLE_PODCASTS: u64  = 0x5A314_4150;
const DITHER_SEED_TIDAL: u64          = 0x5A314_5449;
const DITHER_SEED_AMAZON: u64         = 0x5A314_414D;
const DITHER_SEED_BROADCAST: u64      = 0x5A314_4243;

/// Spotify (-14 LUFS, Gentle/Tape/Air, 4× OS, 16-bit dither).
pub const SPOTIFY: MasteringPreset = MasteringPreset {
    lufs_target:    -14.0,
    true_peak_ceil: -1.0,
    compression:    CompressionStyle::Gentle,
    saturation:     SaturationStyle::Tape,
    eq_curve:       EqCurve::Air,
    limiter:        LimiterAlgorithm::Transparent,
    gain_budget_db: DEFAULT_GAIN_BUDGET_DB,
    dither_bits:    16,
    dither_seed:    DITHER_SEED_SPOTIFY,
    oversampling:   4,
};

/// YouTube (-14 LUFS, Medium/Flat/Punchy, 4× OS, 16-bit dither).
pub const YOUTUBE: MasteringPreset = MasteringPreset {
    lufs_target:    -14.0,
    true_peak_ceil: -1.0,
    compression:    CompressionStyle::Medium,
    saturation:     SaturationStyle::None,
    eq_curve:       EqCurve::Flat,
    limiter:        LimiterAlgorithm::Punchy,
    gain_budget_db: DEFAULT_GAIN_BUDGET_DB,
    dither_bits:    16,
    dither_seed:    DITHER_SEED_YOUTUBE,
    oversampling:   4,
};

/// Apple Music (-16 LUFS, Transparent/Tape/Air, 4× OS, 16-bit dither).
pub const APPLE_MUSIC: MasteringPreset = MasteringPreset {
    lufs_target:    -16.0,
    true_peak_ceil: -1.0,
    compression:    CompressionStyle::Transparent,
    saturation:     SaturationStyle::Tape,
    eq_curve:       EqCurve::Air,
    limiter:        LimiterAlgorithm::Transparent,
    gain_budget_db: DEFAULT_GAIN_BUDGET_DB,
    dither_bits:    16,
    dither_seed:    DITHER_SEED_APPLE_MUSIC,
    oversampling:   4,
};

/// Apple Podcasts (-16 LUFS, Medium/Presence/Transparent, 2× OS, 16-bit dither).
pub const APPLE_PODCASTS: MasteringPreset = MasteringPreset {
    lufs_target:    -16.0,
    true_peak_ceil: -1.0,
    compression:    CompressionStyle::Medium,
    saturation:     SaturationStyle::None,
    eq_curve:       EqCurve::Presence,
    limiter:        LimiterAlgorithm::Transparent,
    gain_budget_db: DEFAULT_GAIN_BUDGET_DB,
    dither_bits:    16,
    dither_seed:    DITHER_SEED_APPLE_PODCASTS,
    oversampling:   2,
};

/// Tidal (-14 LUFS, Transparent/Tape/Flat, 8× OS, 16-bit dither).
pub const TIDAL: MasteringPreset = MasteringPreset {
    lufs_target:    -14.0,
    true_peak_ceil: -1.0,
    compression:    CompressionStyle::Transparent,
    saturation:     SaturationStyle::Tape,
    eq_curve:       EqCurve::Flat,
    limiter:        LimiterAlgorithm::Transparent,
    gain_budget_db: DEFAULT_GAIN_BUDGET_DB,
    dither_bits:    16,
    dither_seed:    DITHER_SEED_TIDAL,
    oversampling:   8,
};

/// Amazon Music (-14 LUFS, Gentle/Air/Punchy, 4× OS, 16-bit dither).
pub const AMAZON: MasteringPreset = MasteringPreset {
    lufs_target:    -14.0,
    true_peak_ceil: -1.0,
    compression:    CompressionStyle::Gentle,
    saturation:     SaturationStyle::None,
    eq_curve:       EqCurve::Air,
    limiter:        LimiterAlgorithm::Punchy,
    gain_budget_db: DEFAULT_GAIN_BUDGET_DB,
    dither_bits:    16,
    dither_seed:    DITHER_SEED_AMAZON,
    oversampling:   4,
};

/// Broadcast (-23 LUFS, Transparent/Flat/Brickwall, 4× OS, 24-bit dither).
pub const BROADCAST: MasteringPreset = MasteringPreset {
    lufs_target:    -23.0,
    true_peak_ceil: -1.0,
    compression:    CompressionStyle::Transparent,
    saturation:     SaturationStyle::None,
    eq_curve:       EqCurve::Flat,
    limiter:        LimiterAlgorithm::Brickwall,
    gain_budget_db: DEFAULT_GAIN_BUDGET_DB,
    dither_bits:    24,
    dither_seed:    DITHER_SEED_BROADCAST,
    oversampling:   4,
};

/// Raw (no loudness target, 1× OS, 32-bit — seed=0 permitted).
pub const RAW: MasteringPreset = MasteringPreset {
    lufs_target:    0.0,
    true_peak_ceil: 0.0,
    compression:    CompressionStyle::Transparent,
    saturation:     SaturationStyle::None,
    eq_curve:       EqCurve::Flat,
    limiter:        LimiterAlgorithm::Transparent,
    gain_budget_db: DEFAULT_GAIN_BUDGET_DB,
    dither_bits:    32,
    dither_seed:    0,
    oversampling:   1,
};

impl MasteringPreset {
    /// Minimal preset from legacy `MasteringIntent` (not a platform preset).
    pub fn from_intent(export_16bit: bool, seed: u64, target_lufs: Option<f32>) -> Self {
        Self {
            lufs_target:    target_lufs.unwrap_or(0.0),
            true_peak_ceil: -1.0,
            compression:    CompressionStyle::Transparent,
            saturation:     SaturationStyle::None,
            eq_curve:       EqCurve::Flat,
            limiter:        LimiterAlgorithm::Transparent,
            gain_budget_db: DEFAULT_GAIN_BUDGET_DB,
            dither_bits:    if export_16bit { 16 } else { 24 },
            dither_seed:    seed,
            oversampling:   4,
        }
    }
}
