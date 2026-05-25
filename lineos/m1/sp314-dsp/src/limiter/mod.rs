// src/limiter/mod.rs

pub mod delay;
pub mod envelope;
pub mod core;

pub use core::{BrickwallLimiter, LimiterConfig};
pub use envelope::{PeakFollower, DEFAULT_CEILING_LINEAR, DECAY_FLOOR_DB};
