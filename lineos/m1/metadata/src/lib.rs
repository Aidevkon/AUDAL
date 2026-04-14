//! LineOS Metadata — structured report generation.
//! Authority: LineOS Constitution v2.0 §07 (M1 Services — comparator rule)
//!
//! COMPARATOR RULE: reads Ebu128Measurement from telemetry — never raw audio.
//! Generates structured JSON BMR-128 reports and EBU reports.
//! Thresholds always from bmr-128.schema.json — never hardcoded.

pub mod bmr128;
pub mod ebu_report;
pub mod project_manifest;
