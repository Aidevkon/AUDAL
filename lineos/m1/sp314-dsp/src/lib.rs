//! sp314-dsp — LineOS Audio Mastering Engine
//! LineOS Constitution v2.0 §05
//! Single source of DSP truth. Immutable between phase releases.
//! no_std + alloc. libm-only float math. XorShiftRng inline.
//! ML Origin Rule: pure algorithmic DSP — no ML weights permitted.

#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod analysis;
pub mod dsp;
pub mod pipeline;
pub mod types;
