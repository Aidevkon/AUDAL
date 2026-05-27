// aether/tuning/mod.rs — S-008 Auto-Tuning Engine
// Authority: spec/locked/S-008_autotuning_engine.md v1.0

pub mod types;
pub mod engine;

pub use types::{DeviationVector, AteResult,
                ATE_CONVERGENCE_TILT, ATE_CONVERGENCE_BODY,
                ATE_CONVERGENCE_TRANS, ATE_TILT_TO_WARMTH,
                ATE_BODY_TO_WARMTH, ATE_TRANS_TO_PUNCH,
                ATE_LOUD_TO_FORWARDNESS, ATE_TILT_TO_FORWARDNESS};
pub use engine::AteEngine;
