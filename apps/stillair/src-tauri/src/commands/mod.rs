pub mod coach;    // Phase 8: Aether Coach
pub mod export;
pub mod insights;
pub mod mastering;
pub mod playback; // Phase 12A: xaak playback (A-003 §8)
pub mod report;   // Phase 13B: BMR-128 PDF report (printpdf, MIT)
pub mod session;  // P9-008: Session State Unification — Phase 11 Dioxus entry point
pub mod visualization;
pub mod telemetry; // Phase 14: getVisualizationData — precomputed SVG paths

// Re-export AudioMeta from mastering for use by insights/export modules
pub use mastering::AudioMeta;
// Re-export session types for Phase 11 Dioxus Cockpit
pub use session::{ComplianceJson, SessionStateJson};
