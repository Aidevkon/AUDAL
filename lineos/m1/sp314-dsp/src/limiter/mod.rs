// src/limiter/mod.rs

pub mod clipper;
pub mod core;
pub mod delay;
pub mod envelope;
pub mod midside;
pub mod true_peak;

pub use clipper::OversampledSoftClipper;
pub use core::{BrickwallLimiter, LimiterConfig};
pub use envelope::{PeakFollower, DECAY_FLOOR_DB, DEFAULT_CEILING_LINEAR};
pub use midside::MidSideProcessor;
pub use true_peak::TruePeakDetector;
