// integration/src/config.rs — DspConfig and constitutional bounds
// Authority: spec/locked/S-009_integration_firewall.md v1.0

// Constitutional DSP bounds — cannot be exceeded regardless of Aether output
use aether::markov::predictive::InstrumentDeltas;
pub const CFW_EQ_GAIN_MIN_DB: f32 = -18.0;
pub const CFW_EQ_GAIN_MAX_DB: f32 = 12.0;
pub const CFW_EQ_FREQ_MIN_HZ: f32 = 20.0;
pub const CFW_EQ_FREQ_MAX_HZ: f32 = 20_000.0;
pub const CFW_EQ_Q_MIN: f32 = 0.1;
pub const CFW_EQ_Q_MAX: f32 = 10.0;
pub const CFW_COMP_THRESHOLD_MIN_DB: f32 = -60.0;
pub const CFW_COMP_THRESHOLD_MAX_DB: f32 = 0.0;
pub const CFW_COMP_RATIO_MIN: f32 = 1.0;
pub const CFW_COMP_RATIO_MAX: f32 = 20.0;
pub const CFW_COMP_ATTACK_MIN_MS: f32 = 0.1;
pub const CFW_COMP_ATTACK_MAX_MS: f32 = 200.0;
pub const CFW_COMP_RELEASE_MIN_MS: f32 = 10.0;
pub const CFW_COMP_RELEASE_MAX_MS: f32 = 2000.0;
pub const CFW_SAT_DRIVE_MIN: f32 = 0.0;
pub const CFW_SAT_DRIVE_MAX: f32 = 1.0;
pub const CFW_SAT_MIX_MIN: f32 = 0.0;
pub const CFW_SAT_MIX_MAX: f32 = 1.0;
pub const CFW_STEREO_WIDTH_MIN: f32 = 0.5;
pub const CFW_STEREO_WIDTH_MAX: f32 = 1.5;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspConfig {
    pub eq: DspEqConfig,
    pub dynamics: DspDynamicsConfig,
    pub sat: DspSatConfig,
    pub stereo: DspStereoConfig,
    pub ambience: Option<DspAmbienceConfig>,
    pub persona_id: String,
    pub chaos_seed: u64,
    #[serde(default)]
    pub instrument_deltas: InstrumentDeltas,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspEqConfig {
    pub low_shelf_gain_db: f32,
    pub low_shelf_freq_hz: f32,
    pub high_shelf_gain_db: f32,
    pub high_shelf_freq_hz: f32,
    /// Zone adjustment bands from S-007.
    /// Preserved in center_hz-sorted order (S-007 guarantees sort).
    pub zone_bands: Vec<ZoneBand>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ZoneBand {
    pub center_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    /// Provenance: which subsystem produced this
    /// band (mirrors EqSource from the semantic
    /// layer). #[serde(default)] keeps old payloads
    /// deserializable.
    #[serde(default)]
    pub source: aether::semantic::zone::EqSource,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspDynamicsConfig {
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
    pub comp_attack_ms: f32,
    pub comp_release_ms: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspSatConfig {
    pub drive: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspStereoConfig {
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspAmbienceConfig {
    pub reverb_time_delta_s: f32,
    pub pre_delay_delta_ms: f32,
    pub diffusion_delta: f32,
    pub high_shelf_gain_db: f32,
    pub high_shelf_freq_delta: f32,
    pub low_shelf_cut_db: f32,
    pub reverb_send_level: f32,
    pub decorrelation: f32,
    pub side_gain_db: f32,
    pub phase_variance: f32,
    pub mono_comp_shelf_db: f32,
    pub hf_damping_db: f32,
    pub low_mid_cut_db: f32,
    pub tail_density_delta: f32,
    pub output_gain_db: f32,
    pub hf_tail_cut_db: f32,
}
