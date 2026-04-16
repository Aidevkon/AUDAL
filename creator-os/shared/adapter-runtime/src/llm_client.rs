//! LLM client — dispatch layer.
//! Authority: LLM Adapter Amendment v1.1 §A4
//!
//! `invoke()` is the ONLY permitted LLM call site in Creator OS.
//! All adapters route through this function — never call providers directly.

use crate::providers::ollama;

/// Supported LLM providers.
/// Adding a new provider requires: new providers/*.rs + variant here.
#[derive(Debug, Clone)]
pub enum Provider {
    /// Ollama local inference — phi3.5:3.8b (JSON) or gemma2:9b (narrative).
    /// Production: swap to Lyria provider without adapter code changes.
    Ollama { model: String },
}

/// Invoke an LLM with a prompt. Returns the raw response string.
///
/// Callers MUST call `validate_output()` on the result before returning
/// it outside the Adapter Boundary. Raw output must never cross the
/// Adapter Boundary into a deterministic Engine or Pipeline.
///
/// Authority: LLM Adapter Amendment v1.1 §A3, §A4
pub async fn invoke(provider: &Provider, prompt: &str) -> Result<String, String> {
    match provider {
        Provider::Ollama { model } => ollama::invoke(model, prompt).await,
    }
}
