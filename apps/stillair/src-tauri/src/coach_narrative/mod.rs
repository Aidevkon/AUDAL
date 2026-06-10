//! Aether layer — Tauri process (NOT WASM).
//! Authority: LLM Adapter Amendment v1.1 · Phase 8 task-decomposition P8-003
//!
//! The Aether layer lives in the Tauri process and handles all LLM interaction.
//! The WASM frontend receives only the validated CoachNarrativeJson struct via Tauri IPC.
//! Raw LLM output never crosses the Adapter Boundary into the frontend.

pub mod coach_adapter;

/// The IPC contract sent to the Cockpit Right MFD after coaching.
/// Every field is fully typed — no serde_json::Value.
/// This struct is the Adapter Boundary output — LLM stochasticity ends here.
///
/// Authority: LLM Adapter Amendment v1.1 §A3 (Adapter Boundary)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoachNarrativeJson {
    /// 2–3 sentence plain-language overview of all findings.
    pub summary: String,
    /// Per-finding explanations in plain language. Teacher voice — no DSP values.
    pub explanations: Vec<FindingExplanation>,
    /// Model that produced this narrative — "phi3.5:3.8b" or "gemma2:9b".
    pub model_used: String,
}

/// Explanation for a single issue from CoachFindings.
/// Fields must not expose DSP values or specific processing instructions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FindingExplanation {
    /// Must exactly match an IssueJson.id from the CoachFindings input.
    /// Validated by CoachAdapter.validate_output() — cross-checked against known IDs.
    pub issue_id: String,
    /// Mirrors IssueJson.severity — "info" | "low" | "medium" | "high".
    /// Coach never modifies severity — it only echoes what rule-engine determined.
    pub severity: String,
    /// Short human-readable title for the finding.
    pub title: String,
    /// Why this finding matters to the listener — teacher voice.
    pub why: String,
    /// Directional suggestion only — no specific values, no plugin names.
    pub suggestion: String,
}
