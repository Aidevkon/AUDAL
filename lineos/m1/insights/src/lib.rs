//! LineOS Insights — compliance comparator.
//! Authority: LineOS Constitution v2.0 §07 (M1 Services — comparator rule)
//!
//! COMPARATOR RULE: reads QualityMetrics + Ebu128Measurement.
//! NEVER re-measures audio. NEVER calls DSP code.
//! Evaluates pass/fail against all presets from bmr-128.schema.json.
//! Produces actionable hints for the rule engine.

pub mod evaluator;
pub mod report;
