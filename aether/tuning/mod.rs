// aether/tuning/mod.rs — S-008 Auto-Tuning Engine
// Authority: spec/locked/S-008_autotuning_engine.md v1.0

pub mod engine;
pub mod types;

pub use engine::AteEngine;
pub use types::{
    AteResult, DeviationVector, ATE_BODY_TO_WARMTH, ATE_CONVERGENCE_BODY, ATE_CONVERGENCE_TILT,
    ATE_CONVERGENCE_TRANS, ATE_LOUD_TO_FORWARDNESS, ATE_TILT_TO_FORWARDNESS, ATE_TILT_TO_WARMTH,
    ATE_TRANS_TO_PUNCH,
};
