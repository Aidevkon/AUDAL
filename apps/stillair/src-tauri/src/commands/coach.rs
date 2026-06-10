//! Coach command — get_coach_narrative Tauri IPC command.
//! Authority: LLM Adapter Amendment v1.1 · Phase 8 P8-005
//!
//! This command is the sole entry point for LLM coaching in the Cockpit.
//! It delegates to CoachAdapter which enforces the Adapter Boundary.
//!
//! Provider selection is via COACH_PROVIDER env var — zero code changes to switch:
//!   COACH_PROVIDER=phi   (default) → phi3.5:3.8b — fast, structured JSON
//!   COACH_PROVIDER=gemma           → gemma2:9b   — richer narrative quality
//!
//! Authority: Phase 8 P8-008 (A/B config)

use crate::coach_narrative::coach_adapter::CoachAdapter;
use crate::coach_narrative::CoachNarrativeJson;
use crate::commands::insights::CoachFindingsJson;
use tauri::command;

/// Generate a plain-language coaching narrative from CoachFindings.
///
/// Input:  CoachFindingsJson — the validated findings from evaluate_findings.
/// Output: CoachNarrativeJson — schema-validated narrative (Adapter Boundary output).
///
/// This command is non-fatal for the Cockpit — if coaching fails (Ollama down,
/// model timeout, JSON validation error), the UI shows findings without narrative.
///
/// FORBIDDEN: Frontend must not pass raw Ollama responses to any Engine.
/// ENFORCED: validate_output() is always called inside CoachAdapter.generate().
#[command]
pub async fn get_coach_narrative(
    findings: CoachFindingsJson,
) -> Result<CoachNarrativeJson, String> {
    // A/B switching: COACH_PROVIDER env var — phi (default) or gemma
    // Authority: Phase 8 P8-008
    let provider_name = std::env::var("COACH_PROVIDER").unwrap_or_else(|_| "phi".into());

    let adapter = match provider_name.as_str() {
        "gemma" => CoachAdapter::gemma(),
        _ => CoachAdapter::phi(), // default: phi3.5:3.8b
    };

    adapter
        .generate(&findings)
        .await
        .map_err(|e| format!("Coach narrative unavailable: {e}"))
}
