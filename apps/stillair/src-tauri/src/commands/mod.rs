pub mod coach;    // Phase 8: Aether Coach
pub mod export;
pub mod insights;
pub mod mastering;
pub mod session;  // P9-008: Session State Unification — Phase 11 Dioxus entry point

// Re-export AudioMeta from mastering for use by insights/export modules
pub use mastering::AudioMeta;
// Re-export session types for Phase 11 Dioxus Cockpit
pub use session::{ComplianceJson, SessionStateJson};
