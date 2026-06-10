#![allow(clippy::doc_lazy_continuation)]
//! LineOS Rule Engine — deterministic coach-core.
//! Authority: LineOS Constitution v2.0 §07 · Coach-Core README v1.0
//!
//! DETERMINISM GUARANTEE: same AnalysisReport + Thresholds → identical CoachFindings.
//! No LLM, no randomness, no side effects, no global state.
//!
//! Public API:
//!   - `evaluate(&AnalysisReport, &Thresholds) → CoachFindings`
//!   - `Thresholds::from_schema(&Bmr128Schema)`
//!   - All types: AnalysisReport, QualityMetrics, ComplianceFlags,
//!    CoachFindings, Issue, IssueParams, Severity

pub mod evaluator;
pub mod rules;
pub mod thresholds;
pub mod types;

// Flatten the most-used items to crate root for ergonomic external use
pub use evaluator::evaluate;
pub use thresholds::Thresholds;
pub use types::{
    AnalysisReport, CoachFindings, ComplianceFlags, Issue, IssueParams, QualityMetrics, Severity,
};
