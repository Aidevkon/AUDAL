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
//! ```rust,no_run
//! use sp314_dsp::pipeline::presets::MasteringTarget;
//! use sp314_dsp::pipeline::engine::Sp314MasteringEngine;
//!
//! let target = MasteringTarget::SpotifyV3;
//! let mut engine = Sp314MasteringEngine::new(target.engine_config(48000), 48000).unwrap();
//!
//! let mut left  = vec![0.0f32; 48000];
//! let mut right = vec![0.0f32; 48000];
//! engine.process_offline(&mut left, &mut right);
//! ```
//!
//! ## Signal Chain
//! ```text
//! input → restoration → EQ → harmonic → compress → limit → meter → output
//! ```

pub mod psychoacoustic;
pub mod masking_eq;
pub mod harmonic;
pub mod compressor;
pub mod pipeline;
pub mod limiter;
pub mod metering;
#[cfg(all(not(target_arch = "wasm32"), feature = "cli"))]
pub mod io;
#[cfg(all(not(target_arch = "wasm32"), feature = "cli"))]
pub mod realtime;
pub mod restoration;
pub mod stft;
