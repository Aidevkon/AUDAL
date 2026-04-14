//! LineOS Telemetry — EBU R128 full measurement service.
//! Authority: LineOS Constitution v2.0 §07 (M1 Services — comparator rule)
//!
//! COMPARATOR RULE: This service reads from Golden Blob only.
//! It NEVER re-measures raw input audio.
//! DSP computation is sp314-dsp's responsibility exclusively.
//!
//! Adds Phase 3 measurements:
//! - LRA (loudness range) via short-term window analysis
//! - Momentary LUFS (last 400ms window)
//! - Short-term LUFS (last 3s window)

#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod lra;
pub mod report;
pub mod windows;
