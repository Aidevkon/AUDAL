//! # lineos-types
//!
//! Shared types for the LineOS M1 ecosystem.
//! Dependency-free (only serde) — safe to use in any crate.
//!
//! ## Migration from sp314-dsp v2.9
//! | v2.9 type | v3 equivalent |
//! |-----------|---------------|
//! | `AudioChunk` | `StereoBuffer` (alias: `AudioChunk`) |
//! | `Ebu128Measurement` | `LufsReport` (alias: `Ebu128Measurement`) |
//! | `GoldenBlob` | `GoldenBlob` (same name, new fields) |
//! | `Bmr128Schema` | `LoudnessTarget` (alias: `Bmr128Schema`) |
//! | `MasteringIntent` | `MasteringIntent` (same name, new fields) |
//! | `MasteringPipeline` | deprecated — use pipelineforge |

pub mod audio;
pub mod config;
pub mod errors;
pub mod golden_blob;
pub mod mastering;
pub mod metrics;
pub mod analysis;
pub mod pre_analysis;
pub mod jini;
pub mod onboarding;
pub mod telemetry;
pub use telemetry::RealtimeFrame;

// Convenience re-exports for migration
pub use analysis::{StemFeatures, StemMetrics, MixMetrics};
pub use pre_analysis::{PreAnalysisData, ZoneActivationFlags};
pub use audio::{AudioChunk, StereoBuffer};
pub use config::{Bmr128Schema, LoudnessTarget, PipelineConstants, PresetThresholds};
pub use golden_blob::{BlobType, GoldenBlob, GoldenInputProfile};
pub use mastering::MasteringIntent;
pub use metrics::{Ebu128Measurement, LufsReport};
pub use jini::{
    // Behaviour enums
    LoudnessBehaviour, SpectralBehaviour, DynamicsBehaviour, StereoBehaviour, QualityBehaviour,
    // Domain enums
    FlavourId, SpinoffTarget, StemKind, MacroHandle,
    // Core JINI types
    BehaviourVector, MacroState, JiniPersonaId, JiniContext, JiniInput,
    JiniSuggestion, JiniAction, PersonaSchema, VocabLevel,
    // Constants
    SCHEMA_BEGINNER, SCHEMA_INTERMEDIATE, SCHEMA_PRO,
    OLLAMA_TIMEOUT_MS, OLLAMA_ENDPOINT, GEMMA_MODEL,
};
pub use onboarding::{
    WizardState, Vision, TasteProfile, PlatformTarget, OnboardingState,
};
