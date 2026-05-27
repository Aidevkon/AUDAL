// aether/chaos/delta.rs — ChaosDelta and constants
// Authority: spec/locked/S-006_chaos_engine.md v1.0

pub const CHAOS_R:            f32 = 3.9;
pub const CHAOS_WARMUP_ITERS: u32 = 100;
pub const CHAOS_WIDTH_MAX:    f32 = 0.10;
pub const CHAOS_DRIVE_MAX_DB: f32 = 1.0;
pub const CHAOS_RELEASE_MAX:  f32 = 10.0;
pub const CHAOS_ATTACK_MAX:   f32 = 2.0;
pub const CHAOS_SHIMMER_MAX:  f32 = 0.5;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChaosDelta {
    pub stereo_width_mod: f32,  // [-CHAOS_WIDTH_MAX,    +CHAOS_WIDTH_MAX]
    pub sat_drive_mod_db: f32,  // [-CHAOS_DRIVE_MAX_DB, +CHAOS_DRIVE_MAX_DB]
    pub comp_release_mod: f32,  // [-CHAOS_RELEASE_MAX,  +CHAOS_RELEASE_MAX]
    pub comp_attack_mod:  f32,  // [-CHAOS_ATTACK_MAX,   +CHAOS_ATTACK_MAX]
    pub air_shimmer_db:   f32,  // [-CHAOS_SHIMMER_MAX,  +CHAOS_SHIMMER_MAX]
                                // HIGH-SHELF GAIN ONLY — not freq
}

impl ChaosDelta {
    pub fn zero() -> Self {
        Self {
            stereo_width_mod: 0.0,
            sat_drive_mod_db: 0.0,
            comp_release_mod: 0.0,
            comp_attack_mod:  0.0,
            air_shimmer_db:   0.0,
        }
    }
}

impl Default for ChaosDelta {
    fn default() -> Self { Self::zero() }
}
