//! # sp314-dsp v3.0.0
//!
//! LineOS M1 Deterministic Mastering Engine.
//!
//! ## Constitutional Guarantees
//! - Same input + seed → bit-identical output on x86_64, aarch64, and macOS
//! - `libm` only for all DSP math — no `std::f32` in the signal path
//! - Zero allocation in the real-time processing loop
//! - No ML weights, no network calls, no external processes
//!
//! ## Quick Start
//! ```rust,ignore
//! // Note: full DspGraph usage requires
//! // m0-daemon's MasteringIntent + topology
//! // JSON — see lineos/m0/m0-daemon for the
//! // production entry point. This crate
//! // (sp314-dsp) provides DSP primitives
//! // consumed by sp314-nodes::DspGraph.
//! ```
//!
//! ## Signal Chain
//! ```text
//! input → restoration → EQ → harmonic → compress → limit → meter → output
//! ```

pub mod compressor;
pub mod harmonic;
#[cfg(all(not(target_arch = "wasm32"), feature = "cli"))]
pub mod io;
pub mod limiter;
pub mod masking_eq;
pub mod metering;
pub mod ola_buffer;
pub mod pipeline;
pub mod psychoacoustic;
#[cfg(all(not(target_arch = "wasm32"), feature = "cli"))]
pub mod realtime;
pub mod restoration;
pub mod stft;
pub mod verification;

pub mod analysis;
pub use analysis::{MixMetrics, PreAnalyzer, StemFeatureAnalyzer, StemFeatures, StemMetrics};

pub mod cut_heal;
pub mod jini;
pub mod spatial;
pub mod transforms;
