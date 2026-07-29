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

pub mod analysis;
pub mod audio;
pub mod config;
pub mod errors;
pub mod golden_blob;
pub mod jini;
pub mod mastering;
pub mod metrics;
pub mod onboarding;
pub mod pre_analysis;
pub mod presets;
pub mod telemetry;
pub use telemetry::RealtimeFrame;

// Convenience re-exports for migration
pub use analysis::{MixMetrics, StemFeatures, StemMetrics};
pub use audio::{AudioChunk, AudioPayload, ManagedPcm, StereoBuffer};
pub use config::{Bmr128Schema, LoudnessTarget, PipelineConstants, PresetThresholds};
pub use golden_blob::{BlobType, GoldenBlob, GoldenInputProfile};
pub use jini::{
    // Core JINI types
    BehaviourVector,
    DynamicsBehaviour,
    // Domain enums
    FlavourId,
    JiniAction,
    JiniContext,
    JiniInput,
    JiniPersonaId,
    JiniSuggestion,
    // Behaviour enums
    LoudnessBehaviour,
    MacroHandle,
    MacroState,
    PersonaSchema,
    QualityBehaviour,
    SpectralBehaviour,
    SpinoffTarget,
    StemKind,
    StereoBehaviour,
    VocabLevel,
    GEMMA_MODEL,
    OLLAMA_ENDPOINT,
    OLLAMA_TIMEOUT_MS,
    // Constants
    SCHEMA_BEGINNER,
    SCHEMA_INTERMEDIATE,
    SCHEMA_PRO,
};
pub use mastering::MasteringIntent;
pub use metrics::{Ebu128Measurement, LufsReport};
pub use onboarding::{OnboardingState, PlatformTarget, TasteProfile, Vision, WizardState};
pub use pre_analysis::{Genre, PreAnalysisData, ZoneActivationFlags};
pub use presets::{ContentKind, DeliverySpec, PresetEntry};
