//! Mastering orchestration module.
//!
//! Responsible for:
//! - Pass 1 timeline building
//! - Pass 2 streaming render
//!
//! Fully decoupled from file I/O.
pub mod decode_provider;
pub mod pass1_pipeline;
pub mod raw_pcm_source;
pub mod seekable_provider;
pub mod sparse_scout;
pub mod streaming_pipeline;
pub mod trunk_pass;
