// integration/src/lib.rs — Integration Firewall crate
// Authority: spec/locked/S-009_integration_firewall.md v1.0

pub mod config;
pub mod error;
pub mod firewall;
pub mod overrides;
pub mod proof_log;

pub use overrides::DspOverrides;

pub use config::{
    DspConfig, DspDynamicsConfig, DspEqConfig, DspSatConfig, DspStereoConfig, ZoneBand,
    CFW_COMP_ATTACK_MAX_MS, CFW_COMP_ATTACK_MIN_MS, CFW_COMP_RATIO_MAX, CFW_COMP_RATIO_MIN,
    CFW_COMP_RELEASE_MAX_MS, CFW_COMP_RELEASE_MIN_MS, CFW_COMP_THRESHOLD_MAX_DB,
    CFW_COMP_THRESHOLD_MIN_DB, CFW_EQ_FREQ_MAX_HZ, CFW_EQ_FREQ_MIN_HZ, CFW_EQ_GAIN_MAX_DB,
    CFW_EQ_GAIN_MIN_DB, CFW_EQ_Q_MAX, CFW_EQ_Q_MIN, CFW_SAT_DRIVE_MAX, CFW_SAT_DRIVE_MIN,
    CFW_SAT_MIX_MAX, CFW_SAT_MIX_MIN, CFW_STEREO_WIDTH_MAX, CFW_STEREO_WIDTH_MIN,
};
pub use error::FirewallError;
pub use firewall::IntegrationFirewall;
pub use proof_log::{ClampEvent, ProofLog};
