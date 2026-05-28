//! CoachAdapter — LLM Adapter for audio coaching narrative.
//! Authority: LLM Adapter Amendment v1.1 · Phase 8 P8-004 · Phase 9 P9-006
//!
//! Phase 9: Prompt loaded from assets/coach_prompt.toml at runtime.
//! Edit coach_prompt.toml without rebuilding to adjust tone, rules, examples.
//! Falls back to include_str! embedded copy if asset file not found.
//!
//! Architecture (binding):
//!   CoachFindings (read-only input)
//!       → load_prompt_config()              [runtime TOML load]
//!       → build_prompt()                    [teacher identity from config]
//!       → adapter_runtime::llm_client::invoke()   [sole LLM call point]
//!       → validate_output()                 [Adapter Boundary]
//!       → CoachNarrativeJson                [deterministic from here]
//!
//! FORBIDDEN (per Amendment §A6):
//!   ❌ Calling Ollama HTTP directly (use adapter_runtime::llm_client::invoke)
//!   ❌ Passing raw LLM output to any caller
//!   ❌ Modifying CoachFindings severity or issues
//!   ❌ Adding new issues not present in findings
//!   ❌ Giving specific DSP values in narrative

use crate::commands::insights::CoachFindingsJson;
use super::{CoachNarrativeJson, FindingExplanation};
use adapter_runtime::llm_client::{invoke, Provider};

// ── Prompt configuration structs (deserialized from coach_prompt.toml) ────────

#[derive(serde::Deserialize, Clone)]
struct PromptConfig {
    identity: IdentityConfig,
    output:   OutputConfig,
    schema:   SchemaConfig,
    examples: ExamplesConfig,
}

#[derive(serde::Deserialize, Clone)]
struct IdentityConfig {
    role:  String,
    style: String,
    rules: Vec<String>,
}

#[derive(serde::Deserialize, Clone)]
struct OutputConfig {
    format:      String,
    no_markdown: bool,
    #[allow(dead_code)]
    no_preamble: bool,
}

#[derive(serde::Deserialize, Clone)]
struct SchemaConfig {
    template: String,
}

#[derive(serde::Deserialize, Clone)]
struct ExamplesConfig {
    good: Vec<ExampleEntry>,
}

#[derive(serde::Deserialize, Clone)]
struct ExampleEntry {
    issue_id:   String,
    severity:   String,
    title:      String,
    why:        String,
    suggestion: String,
}

// Embedded fallback — compile-time guarantee that the file exists.
// If the runtime asset path fails, this is used instead.
const DEFAULT_PROMPT_TOML: &str =
    include_str!("../../assets/coach_prompt.toml");

// ── CoachAdapter ──────────────────────────────────────────────────────────────

/// Coach adapter — translates CoachFindings to plain-language CoachNarrative.
///
/// Teacher identity (immutable):
/// - Explains WHY findings matter to the listener
/// - Never gives specific DSP values
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

    /// Load prompt configuration from assets/coach_prompt.toml.
    /// Runtime load: changes take effect on next invocation, no rebuild needed.
    /// Falls back to embedded include_str! copy if asset file not found.
    fn load_prompt_config() -> PromptConfig {
        // Resolve asset path relative to the running binary
        let asset_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("assets/coach_prompt.toml")))
            .unwrap_or_else(|| std::path::Path::new("assets/coach_prompt.toml").to_path_buf());

        let content_bytes = match std::fs::read(&asset_path) {
            Ok(b)  => {
                eprintln!("[CoachAdapter] loaded prompt config from {:?}", asset_path);
                b
            }
            Err(_) => {
                eprintln!(
                    "[CoachAdapter] asset {:?} not found — using embedded fallback",
                    asset_path
                );
                DEFAULT_PROMPT_TOML.as_bytes().to_vec()
            }
        };

        toml::from_str(
            std::str::from_utf8(&content_bytes).unwrap_or(DEFAULT_PROMPT_TOML)
        ).unwrap_or_else(|e| {
            eprintln!("[CoachAdapter] TOML parse error ({e}) — using embedded fallback");
            toml::from_str(DEFAULT_PROMPT_TOML)
                .expect("embedded DEFAULT_PROMPT_TOML must always be valid TOML")
        })
    }

    /// Generate a CoachNarrative from CoachFindings.
    ///
    /// Flow:
    ///  1. load_prompt_config   — runtime TOML, fallback to embedded
    ///  2. build_prompt         — translate findings + config to LLM prompt
    ///  3. adapter_runtime invoke — sole LLM call point
    ///  4. validate_output      — parse + validate (Adapter Boundary)
    ///  5. return typed CoachNarrativeJson
    pub async fn generate(
        &self,
        findings: &CoachFindingsJson,
    ) -> Result<CoachNarrativeJson, String> {
        let config = Self::load_prompt_config();
        let prompt = self.build_prompt(findings, &config);
        // Step 3: invoke via adapter-runtime — never call Ollama directly
        let raw = invoke(&self.provider, &prompt).await?;
        // Step 4: validate_output — Adapter Boundary. Raw output stops here.
        self.validate_output(&raw, findings)
    }

    /// Build a prompt using identity + rules from coach_prompt.toml.
    /// Teacher identity enforced — no DSP instructions permitted.
    fn build_prompt(&self, findings: &CoachFindingsJson, config: &PromptConfig) -> String {
        let rules_text   = config.identity.rules
            .iter()
            .map(|r| format!("- {r}"))
            .collect::<Vec<_>>()
            .join("\n");

        let examples_text = config.examples.good
            .iter()
            .map(|e| format!(
                "  issue_id: {}, severity: {}, title: {}\n  why: {}\n  suggestion: {}",
                e.issue_id, e.severity, e.title, e.why, e.suggestion
            ))
            .collect::<Vec<_>>()
            .join("\n\n");

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

        let no_markdown_rule = if config.output.no_markdown {
            "No markdown fences. No text before or after the JSON object."
        } else {
            ""
        };

        format!(
            r#"You are a {role}. Your approach: {style}.

Strict rules for your response:
{rules}

Audio findings for this track:
{issues}

Overall assessment: {recommendation}

Few-shot examples of good responses:
{examples}

You MUST respond ONLY with valid {format} matching this EXACT schema:
{schema}

{no_md}"#,
            role           = config.identity.role,
            style          = config.identity.style,
            rules          = rules_text,
            issues         = issues_text,
            recommendation = findings.recommendation,
            examples       = examples_text,
            format         = config.output.format,
            schema         = config.schema.template,
            no_md          = no_markdown_rule,
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
