// aether/tuning/types.rs — ATE types and constants
// Authority: spec/locked/S-008_autotuning_engine.md v1.0
// Single-pass ATE (v1.0). Iterative re-render deferred to v2.0.

use crate::personas::config::PersonaOverride;

// Convergence thresholds
pub const ATE_CONVERGENCE_TILT: f32 = 0.3; // dB
pub const ATE_CONVERGENCE_BODY: f32 = 0.25; // dB
pub const ATE_CONVERGENCE_TRANS: f32 = 0.15; // normalized

// Mapping weights (RFC-004 §4.5)
pub const ATE_TILT_TO_WARMTH: f32 = 0.5;
pub const ATE_BODY_TO_WARMTH: f32 = 0.4;
pub const ATE_TRANS_TO_PUNCH: f32 = 0.3;
pub const ATE_LOUD_TO_FORWARDNESS: f32 = 0.3;
pub const ATE_TILT_TO_FORWARDNESS: f32 = 0.2;
// Smoothness not mapped in v1.0 (no reliable metric)
// Width belongs to S-006 (Chaos Engine)

/// Single-pass deviation: reference_features - output_features.
/// Positive = output below reference (needs more).
/// Negative = output above reference (needs less).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeviationVector {
    /// Spectral tilt dB — empirical log ratio of mix centroids
    pub delta_tilt_db: f32,
    /// Low-mid body dB — bass RMS difference
    pub delta_body_db: f32,
    /// Transient density — drums crest factor diff, normalized [-1,1]
    pub delta_transients: f32,
    /// Integrated LUFS — loudness difference
    pub delta_loudness_lufs: f32,
    /// True if all deltas within convergence thresholds
    pub converged: bool,
}

impl DeviationVector {
    pub fn is_converged(&self) -> bool {
        self.delta_tilt_db.abs() < ATE_CONVERGENCE_TILT
            && self.delta_body_db.abs() < ATE_CONVERGENCE_BODY
            && self.delta_transients.abs() < ATE_CONVERGENCE_TRANS
    }

    pub fn zero() -> Self {
        let dv = Self {
            delta_tilt_db: 0.0,
            delta_body_db: 0.0,
            delta_transients: 0.0,
            delta_loudness_lufs: 0.0,
            converged: false,
        };
        Self {
            converged: dv.is_converged(),
            ..dv
        }
    }
}

/// Output of a single ATE pass.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AteResult {
    pub deviation: DeviationVector,
    pub persona_override: PersonaOverride,
    pub converged: bool,
}
