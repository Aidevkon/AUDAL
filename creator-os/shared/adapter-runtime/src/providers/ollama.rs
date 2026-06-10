//! Ollama local inference provider.
//! Authority: LLM Adapter Amendment v1.1 §A4
//!
//! Used for dev/prod with local models: phi3.5:3.8b (JSON), gemma2:9b (narrative).
//! Production: swap to Lyria 3 provider without code changes — just add a new provider.
//!
//! IMPORTANT: This module must only be called via `llm_client::invoke()`.
//! No adapter or command may import from this module directly.

use serde::{Deserialize, Serialize};

const OLLAMA_BASE: &str = "http://localhost:11434";

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: u32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

/// Call Ollama with a prompt. Returns the raw response string.
///
/// - `stream: false` — wait for full response (adapter-runtime pattern).
/// - `temperature: 0.3` — low temperature for consistent, structured output.
/// - `num_predict: 512` — sufficient for coach narratives.
/// - Timeout: 120s — local models with 9B parameters need headroom.
///
/// Callers must be `llm_client::invoke()` exclusively.
pub async fn invoke(model: &str, prompt: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let req = OllamaRequest {
        model,
        prompt,
        stream: false,
        options: OllamaOptions {
            temperature: 0.3,
            num_predict: 512,
        },
    };
    let resp = client
        .post(format!("{}/api/generate", OLLAMA_BASE))
        .json(&req)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| format!("Ollama unreachable: {e}"))?;

    let body: OllamaResponse = resp
        .json()
        .await
        .map_err(|e| format!("Ollama parse error: {e}"))?;

    Ok(body.response)
}
