// aether/mapping/mod.rs — S-005 Macro → Micro Mapping
// Authority: spec/locked/S-005_macro_micro_mapping.md v1.0

pub mod ambience;
pub mod curves;
pub mod mapper;
pub mod types;

pub use curves::apply_curve;
pub use mapper::MacroMicroMapper;
pub use types::{
    DynamicsDelta, EqDelta, MicroDelta, SaturationDelta, StereoDelta, COMP_ATTACK_DELTA_MAX_MS,
    COMP_ATTACK_DELTA_MIN_MS, COMP_RATIO_DELTA_MAX, COMP_RATIO_DELTA_MIN,
    COMP_RELEASE_DELTA_MAX_MS, COMP_RELEASE_DELTA_MIN_MS, COMP_THRESHOLD_DELTA_MAX_DB,
    COMP_THRESHOLD_DELTA_MIN_DB, EQ_FREQ_DELTA_MAX_HZ, EQ_FREQ_DELTA_MIN_HZ, EQ_GAIN_DELTA_MAX_DB,
    EQ_GAIN_DELTA_MIN_DB, SAT_DRIVE_DELTA_MAX, SAT_DRIVE_DELTA_MIN, SAT_MIX_DELTA_MAX,
    SAT_MIX_DELTA_MIN, STEREO_WIDTH_DELTA_MAX, STEREO_WIDTH_DELTA_MIN,
};
