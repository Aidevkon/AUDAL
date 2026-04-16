pub mod coach;    // Phase 8: Aether Coach
pub mod export;
pub mod insights;
pub mod mastering;

// Re-export AudioMeta from mastering for use by insights/export modules
pub use mastering::AudioMeta;
