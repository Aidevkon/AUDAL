//! Types module — public re-exports for rule-engine consumers.

mod analysis_report;
mod coach_findings;

pub use analysis_report::{AnalysisReport, ComplianceFlags, QualityMetrics};
pub use coach_findings::{CoachFindings, Issue, IssueParams, Severity};
