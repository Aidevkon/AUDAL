// aether/chaos/mod.rs — S-006 Chaos Modulation Engine
// Authority: spec/locked/S-006_chaos_engine.md v1.0

pub mod delta;
pub mod seed;
pub mod engine;

pub use delta::{ChaosDelta, CHAOS_R, CHAOS_WARMUP_ITERS,
                CHAOS_WIDTH_MAX, CHAOS_DRIVE_MAX_DB,
                CHAOS_RELEASE_MAX, CHAOS_ATTACK_MAX, CHAOS_SHIMMER_MAX};
pub use engine::ChaosEngine;
pub use seed::build_seed;
