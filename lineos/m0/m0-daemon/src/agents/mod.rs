//! Constitutional Agent Architecture v3.1
//! Authority: Constitutional Agent Architecture Spec v3.1
//!
//! R1 — SchemaAgent:  validates JSON, never generates
//! R2 — Conductor:    orchestrates workflow, builds ExecutionPlan
//! R3 — Executor:     executes plan, calls DspAdapter, pure action
//! R2 — WizardAgent:  telemetry → findings JSON

pub mod operator;
pub mod schema;
pub mod conductor;
pub mod executor;
pub mod wizard;
