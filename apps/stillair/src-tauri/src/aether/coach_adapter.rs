//! CoachAdapter — LLM Adapter for audio coaching narrative.
//! Authority: LLM Adapter Amendment v1.1 · Phase 8 P8-004
//!
//! Architecture (binding):
//!   CoachFindings (read-only input)
//!       → build_prompt()
//!       → adapter_runtime::llm_client::invoke()   [sole LLM call point]
//!       → validate_output()                        [Adapter Boundary]
//!       → CoachNarrativeJson                       [deterministic from here]
//!
//! FORBIDDEN (per Amendment §A6):
//!   ❌ Calling Ollama HTTP directly (use adapter_runtime::llm_client::invoke)
//!   ❌ Passing raw LLM output to any caller
//!   ❌ Modifying CoachFindings severity or issues
//!   ❌ Adding new issues not present in findings
//!   ❌ Giving specific DSP values in narrative
//!   ❌ Retry logic (adapter-runtime handles retries)

use crate::commands::insights::CoachFindingsJson;
use super::{CoachNarrativeJson, FindingExplanation};
use adapter_runtime::llm_client::{invoke, Provider};

/// Coach adapter — translates CoachFindings to plain-language CoachNarrative.
///
/// Teacher identity (immutable):
/// - Explains WHY findings matter to the listener
/// - Never gives specific DSP values (no "reduce by 2dB at 3kHz")
/// - Never modifies severity scores from the rule-engine
/// - Never adds issues beyond what rule-engine found
pub struct CoachAdapter {
    pub provider: Provider,
}

impl CoachAdapter {
    /// Default adapter — phi3.5:3.8b (fast, structured JSON output).
    pub fn phi() -> Self {
        Self { provider: Provider::Ollama { model: "phi3.5:3.8b".into() } }
    }

    /// Narrative-quality adapter — gemma2:9b (richer explanation text).
    pub fn gemma() -> Self {
        Self { provider: Provider::Ollama { model: "gemma2:9b".into() } }
    }

    /// Generate a CoachNarrative from CoachFindings.
    ///
    /// Flow:
    ///  1. build_prompt — translate findings to LLM prompt
    ///  2. adapter_runtime invoke — sole LLM call point
    ///  3. validate_output — parse + validate (Adapter Boundary)
    ///  4. return typed CoachNarrativeJson
    pub async fn generate(
        &self,
        findings: &CoachFindingsJson,
    ) -> Result<CoachNarrativeJson, String> {
        let prompt = self.build_prompt(findings);
        // Step 2: invoke via the adapter-runtime — never call Ollama directly
        let raw = invoke(&self.provider, &prompt).await?;
        // Step 3: validate_output — Adapter Boundary. Raw output stops here.
        self.validate_output(&raw, findings)
    }

    /// Build a prompt with teacher identity — no DSP instructions permitted.
    fn build_prompt(&self, findings: &CoachFindingsJson) -> String {
        let issues_text = if findings.issues.is_empty() {
            "No issues found — the track is fully compliant.".to_string()
        } else {
            findings.issues.iter().map(|i| {
                format!(
                    "- {} [severity: {}]: current={:.1}, target={:.1}, delta={:.1}, tags={:?}",
                    i.id, i.severity, i.current, i.target, i.delta, i.tags
                )
            }).collect::<Vec<_>>().join("\n")
        };

        format!(
            r#"You are a professional audio mastering coach. Your role is to explain
audio analysis findings to an artist in plain, encouraging language.
You are a teacher, not a mixing engineer. Never give specific DSP values
(no "reduce by 2dB", no plugin names, no frequency values).
Explain WHY each finding matters for the listener's experience.
Give directional suggestions only ("the track could benefit from more headroom").

Audio analysis results for this track:
{issues_text}

Overall assessment: {recommendation}

Respond ONLY with valid JSON matching this EXACT schema (no markdown, no preamble):
{{
  "summary": "2-3 sentence plain-language overview",
  "explanations": [
    {{
      "issue_id": "exact_issue_id_from_above",
      "severity": "info|low|medium|high",
      "title": "short human-readable title",
      "why": "why this matters for the listener (1-2 sentences)",
      "suggestion": "what direction to explore (no specific values)"
    }}
  ]
}}

JSON only. No markdown fences. No text before or after the JSON object."#,
            issues_text = issues_text,
            recommendation = findings.recommendation,
        )
    }

    /// Validate the raw LLM response — Adapter Boundary.
    ///
    /// Rules enforced:
    ///  1. Must be valid JSON
    ///  2. Every explanation.issue_id must match a known issue from findings
    ///  3. Returns typed CoachNarrativeJson — raw text never escapes this fn
    ///
    /// On failure: returns Err — never returns partial or raw output.
    fn validate_output(
        &self,
        raw: &str,
        findings: &CoachFindingsJson,
    ) -> Result<CoachNarrativeJson, String> {
        // Strip common LLM markdown fence patterns before parsing
        let clean = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Parse the validated intermediate shape — not public, stays in this fn
        #[derive(serde::Deserialize)]
        struct LlmOutput {
            summary:      String,
            explanations: Vec<FindingExplanation>,
        }

        let parsed: LlmOutput = serde_json::from_str(clean)
            .map_err(|e| format!(
                "CoachAdapter: LLM returned invalid JSON: {e}\nRaw (first 200 chars): {}",
                &clean[..clean.len().min(200)]
            ))?;

        // Validate: every explanation must reference a known issue_id
        // Coach is FORBIDDEN from inventing new issues (Amendment §A6)
        let known_ids: std::collections::HashSet<&str> =
            findings.issues.iter().map(|i| i.id.as_str()).collect();

        for exp in &parsed.explanations {
            if !known_ids.contains(exp.issue_id.as_str()) {
                return Err(format!(
                    "CoachAdapter: LLM invented unknown issue_id '{}'. \
                     Permitted IDs: {:?}",
                    exp.issue_id,
                    known_ids
                ));
            }
        }

        // Extract model name from provider
        let model_used = match &self.provider {
            adapter_runtime::llm_client::Provider::Ollama { model } => model.clone(),
        };

        // Adapter Boundary crossed — return typed, validated struct
        Ok(CoachNarrativeJson {
            summary:      parsed.summary,
            explanations: parsed.explanations,
            model_used,
        })
    }
}
