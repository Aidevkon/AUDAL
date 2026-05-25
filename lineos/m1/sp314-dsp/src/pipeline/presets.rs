use crate::pipeline::engine::EngineConfig;
use crate::limiter::LimiterConfig;
use crate::masking_eq::MaskingEQConfig;
use crate::compressor::stereo::CompressorV3Config;
use crate::compressor::core::CompressorBandConfig;
use crate::restoration::RestorationConfig;

/// Mastering target — determines RMS target and processing intent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MasteringTarget {
    /// Flat EQ, ratio 1:1 — pure gain staging, no color
    Transparent,
    /// Target RMS -14.0 dB — streaming standard
    SpotifyV3,
    /// Target RMS -16.0 dB — voice clarity, low density
    PodcastVoice,
    /// Target RMS -18.0 dB — acoustic, jazz, classical
    ClassicalAcoustic,
    /// Target RMS  -9.0 dB — dense, energetic
    LoudMaster,
    /// Target RMS  -7.0 dB — maximum density
    AggressiveEDM,
    /// Target RMS -23.0 dB — EBU R128, TV/Film/Netflix
    BroadcastVideo,
    /// Target RMS -24.0 dB — US broadcast standard (ATSC A/85)
    AtscA85,
}

impl MasteringTarget {
    /// Target RMS in dB for this preset.
    /// Returns None for Transparent (bypass — no loudness target).
    /// Used by the Autotuner (PROMPT 12) to converge makeup gain.
    pub fn target_lufs(&self) -> Option<f32> {
        match self {
            MasteringTarget::Transparent      => None,
            MasteringTarget::SpotifyV3        => Some(-14.0),
            MasteringTarget::PodcastVoice     => Some(-16.0),
            MasteringTarget::ClassicalAcoustic=> Some(-18.0),
            MasteringTarget::LoudMaster       => Some(-9.0),
            MasteringTarget::AggressiveEDM    => Some(-7.0),
            MasteringTarget::BroadcastVideo   => Some(-23.0),
            MasteringTarget::AtscA85          => Some(-24.0),
        }
    }

    /// User-facing display name — used by UI presentation layer.
    pub fn display_name(&self) -> &'static str {
        match self {
            MasteringTarget::Transparent       => "Natural (No Processing)",
            MasteringTarget::SpotifyV3         => "Streaming (Spotify / Apple)",
            MasteringTarget::PodcastVoice      => "Podcast & Voice",
            MasteringTarget::ClassicalAcoustic => "Classical & Acoustic",
            MasteringTarget::LoudMaster        => "Loud Master (Pop/Rock)",
            MasteringTarget::AggressiveEDM     => "Club & EDM",
            MasteringTarget::BroadcastVideo    => "Broadcast & Film (EBU R128)",
            MasteringTarget::AtscA85           => "US Broadcast (ATSC A/85)",
        }
    }

    /// Build a fully configured EngineConfig for this target.
    pub fn engine_config(&self, sample_rate: u32) -> EngineConfig {
        match self {
            MasteringTarget::Transparent       => transparent_config(sample_rate),
            MasteringTarget::SpotifyV3         => spotify_v3_config(sample_rate),
            MasteringTarget::PodcastVoice      => podcast_voice_config(sample_rate),
            MasteringTarget::ClassicalAcoustic => classical_acoustic_config(sample_rate),
            MasteringTarget::LoudMaster        => loud_master_config(sample_rate),
            MasteringTarget::AggressiveEDM     => aggressive_edm_config(sample_rate),
            MasteringTarget::BroadcastVideo    => broadcast_video_config(sample_rate),
            MasteringTarget::AtscA85           => atsc_a85_config(sample_rate),
        }
    }
}

fn default_band_config(
    threshold_db: f32,
    ratio:        f32,
    attack_ms:    f32,
    release_ms:   f32,
    makeup_db:    f32,
) -> CompressorBandConfig {
    CompressorBandConfig {
        threshold_db,
        ratio,
        knee_db:      2.0,   // always soft knee
        attack_ms,
        release_ms,
        makeup_db,
        crossover_hz: 150.0, // always 150Hz LR4
    }
}

/// Transparent: flat EQ, ratio 1:1, no color.
/// Proves the engine adds nothing when not asked to.
fn transparent_config(_sample_rate: u32) -> EngineConfig {
    EngineConfig {
        eq_config: MaskingEQConfig {
            target_db:      [0.0; 8],  // flat — no EQ
            mask_margin_db: 12.0,       // max allowed by validation, with max_boost=0 never boosts
            max_boost_db:   0.0,
            target_phon:    80.0,
        },
        comp_config: CompressorV3Config {
            mid_config:  default_band_config(-6.0, 1.0, 10.0, 150.0, 0.0),
            side_config: default_band_config(-6.0, 1.0, 10.0, 150.0, 0.0),
        },
        parallel_mix:     0.0,   // 100% dry — compressor bypassed
        target_makeup_db: 0.0,
        limiter_config:   LimiterConfig::default(),
        restoration_config: RestorationConfig::bypass(),
        harmonic_config:  None,
    }
}

/// Spotify V3: streaming-optimized, transparent compression.
fn spotify_v3_config(_sample_rate: u32) -> EngineConfig {
    EngineConfig {
        eq_config: MaskingEQConfig {
            target_db:      [1.0, 0.5, 0.0, 0.0, 0.5, 1.0, 1.5, 1.0],
            mask_margin_db: 3.0,
            max_boost_db:   4.0,
            target_phon:    80.0,
        },
        comp_config: CompressorV3Config {
            mid_config:  default_band_config(-18.0, 2.5, 10.0, 150.0, 1.0),
            side_config: default_band_config(-24.0, 1.8, 20.0, 200.0, 0.0),
        },
        parallel_mix:     0.5,
        target_makeup_db: 0.0,
        limiter_config:   LimiterConfig::default(),
        restoration_config: RestorationConfig::music(),
        harmonic_config:  None,
    }
}

/// Podcast Voice: clarity-focused, low density.
fn podcast_voice_config(_sample_rate: u32) -> EngineConfig {
    EngineConfig {
        eq_config: MaskingEQConfig {
            target_db:      [0.0, 0.5, 1.0, 1.5, 1.0, 0.5, 0.0, 0.0],
            mask_margin_db: 4.0,
            max_boost_db:   3.0,
            target_phon:    80.0,
        },
        comp_config: CompressorV3Config {
            mid_config:  default_band_config(-20.0, 3.0, 5.0, 100.0, 0.5),
            side_config: default_band_config(-28.0, 1.5, 10.0, 150.0, 0.0),
        },
        parallel_mix:     0.1,  // serial compression preferred for voice
        target_makeup_db: 0.0,
        limiter_config:   LimiterConfig::default(),
        restoration_config: RestorationConfig::voice(),
        harmonic_config:  None,
    }
}

/// Loud Master: dense and energetic.
fn loud_master_config(_sample_rate: u32) -> EngineConfig {
    EngineConfig {
        eq_config: MaskingEQConfig {
            target_db:      [2.0, 1.5, 1.0, 0.5, 1.0, 1.5, 2.0, 1.5],
            mask_margin_db: 2.0,
            max_boost_db:   6.0,
            target_phon:    80.0,
        },
        comp_config: CompressorV3Config {
            mid_config:  default_band_config(-16.0, 3.5, 8.0, 120.0, 2.0),
            side_config: default_band_config(-22.0, 2.5, 15.0, 180.0, 0.0),
        },
        parallel_mix:     0.7,
        target_makeup_db: 0.0,
        limiter_config:   LimiterConfig::default(),
        restoration_config: RestorationConfig::music(),
        harmonic_config:  None,
    }
}

/// Aggressive EDM: maximum density, high parallel compression.
fn aggressive_edm_config(_sample_rate: u32) -> EngineConfig {
    EngineConfig {
        eq_config: MaskingEQConfig {
            target_db:      [3.0, 2.0, 1.5, 1.0, 1.5, 2.0, 3.0, 2.0],
            mask_margin_db: 1.5,
            max_boost_db:   8.0,
            target_phon:    80.0,
        },
        comp_config: CompressorV3Config {
            mid_config:  default_band_config(-14.0, 4.0, 5.0, 100.0, 3.0),
            side_config: default_band_config(-20.0, 3.0, 10.0, 150.0, 0.0),
        },
        parallel_mix:     0.85,
        target_makeup_db: 0.0,
        limiter_config:   LimiterConfig::default(),
        restoration_config: RestorationConfig::music(),
        harmonic_config:  None,
    }
}

/// Classical & Acoustic: breathing room, slow attack, minimal parallel.
fn classical_acoustic_config(_sample_rate: u32) -> EngineConfig {
    EngineConfig {
        eq_config: MaskingEQConfig {
            target_db:      [0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 0.5],
            mask_margin_db: 5.0,   // conservative — only boost if clearly masked
            max_boost_db:   3.0,
            target_phon:    80.0,
        },
        comp_config: CompressorV3Config {
            mid_config:  default_band_config(-22.0, 2.0, 30.0, 300.0, 0.5),
            side_config: default_band_config(-30.0, 1.3, 40.0, 400.0, 0.0),
        },
        parallel_mix:     0.15,  // minimal parallel — preserve natural dynamics
        target_makeup_db: 0.0,
        limiter_config:   LimiterConfig::default(),
        restoration_config: RestorationConfig::music(),
        harmonic_config:  None,
    }
}

/// Broadcast & Film: EBU R128 compliant, wide dynamic range.
fn broadcast_video_config(_sample_rate: u32) -> EngineConfig {
    EngineConfig {
        eq_config: MaskingEQConfig {
            target_db:      [0.0, 0.0, 0.5, 1.0, 0.5, 0.0, 0.0, 0.0],
            mask_margin_db: 6.0,   // very conservative
            max_boost_db:   2.0,
            target_phon:    80.0,
        },
        comp_config: CompressorV3Config {
            mid_config:  default_band_config(-24.0, 1.8, 20.0, 250.0, 0.0),
            side_config: default_band_config(-30.0, 1.3, 30.0, 300.0, 0.0),
        },
        parallel_mix:     0.1,   // near-serial — EBU R128 needs wide dynamics
        target_makeup_db: 0.0,
        limiter_config:   LimiterConfig::default(),
        restoration_config: RestorationConfig::voice(),
        harmonic_config:  None,
    }
}

fn atsc_a85_config(_sample_rate: u32) -> EngineConfig {
    EngineConfig {
        eq_config: MaskingEQConfig {
            target_db:      [0.0, 0.0, 0.5, 1.0, 0.5, 0.0, 0.0, 0.0],
            mask_margin_db: 6.0,
            max_boost_db:   2.0,
            target_phon:    80.0,
        },
        comp_config: CompressorV3Config {
            mid_config:  default_band_config(-26.0, 1.8, 20.0, 250.0, 0.0),
            side_config: default_band_config(-32.0, 1.3, 30.0, 300.0, 0.0),
        },
        parallel_mix:     0.1,
        target_makeup_db: 0.0,
        limiter_config:   LimiterConfig::default(),
        restoration_config: RestorationConfig::voice(),
        harmonic_config:  None,
    }
}
