//! JINI Ollama adapter — Gemma 4 via local Ollama.
//! Authority: JINI Spec v1.0 J-P3
//! Primary LLM path. Falls back to rule_based_suggestion() on any error.
//! INV-JINI-5: audio never sent to cloud.
//! INV-JINI-12: timeout 5s — silent fallback, never error shown to user.

pub mod schema_agent;

use lineos_types::{
    BehaviourVector, JiniPersonaId, JiniSuggestion, JiniAction,
    FlavourId, MacroHandle, OLLAMA_ENDPOINT, GEMMA_MODEL, OLLAMA_TIMEOUT_MS,
};
use sp314_dsp::jini::rule_based_suggestion;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize)]
struct OllamaRequest {
    model:  &'static str,
    prompt: String,
    stream: bool,
    format: &'static str,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

#[derive(Deserialize)]
struct LlmOutput {
    narrative: String,
    action:    Option<LlmAction>,
    confidence: f32,
}

#[derive(Deserialize)]
struct LlmAction {
    #[serde(rename = "type")]
    kind:   String,
    handle: Option<String>,
    delta:  Option<f32>,
    reason: Option<String>,
    to:     Option<String>,
}

/// Call Gemma 4 via Ollama. Falls back to rule_based_suggestion on any error.
/// INV-JINI-3: every suggestion validated by Schema Agent before returning.
pub async fn jini_suggest(
    behaviour: &BehaviourVector,
    persona:   &JiniPersonaId,
) -> JiniSuggestion {
    let fallback = rule_based_suggestion(behaviour, persona);
    let suggestion = match ollama_call(behaviour, persona).await {
        Ok(s)  => s,
        Err(_) => return fallback,
    };
    // INV-JINI-3: Schema Agent validates before UI delivery
    schema_agent::validated_or_fallback(suggestion, fallback)
}

async fn ollama_call(
    behaviour: &BehaviourVector,
    persona:   &JiniPersonaId,
) -> Result<JiniSuggestion, Box<dyn std::error::Error>> {
    let prompt = build_prompt(behaviour, persona);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(OLLAMA_TIMEOUT_MS))
        .build()?;

    let req = OllamaRequest {
        model:  GEMMA_MODEL,
        prompt,
        stream: false,
        format: "json",
    };

    let resp: OllamaResponse = client
        .post(OLLAMA_ENDPOINT)
        .json(&req)
        .send()
        .await?
        .json()
        .await?;

    let output: LlmOutput = serde_json::from_str(&resp.response)?;

    // Clamp confidence
    let confidence = output.confidence.clamp(0.0, 1.0);

    // Parse action — fallback to SuggestNothing on parse failure
    let action = parse_action(output.action);

    // Enforce INV-JINI-11: max 500 chars
    let narrative = output.narrative.chars().take(500).collect();

    Ok(JiniSuggestion {
        narrative,
        action,
        confidence,
        persona_used: persona.clone(),
    })
}

fn build_prompt(behaviour: &BehaviourVector, persona: &JiniPersonaId) -> String {
    let schema = match persona {
        JiniPersonaId::Beginner     => lineos_types::SCHEMA_BEGINNER.system_prompt,
        JiniPersonaId::Intermediate => lineos_types::SCHEMA_INTERMEDIATE.system_prompt,
        JiniPersonaId::Pro          => lineos_types::SCHEMA_PRO.system_prompt,
    };

    format!(
        "{}\n\nAudio analysis:\n\
         - Loudness: {:?}\n\
         - Spectral: {:?}\n\
         - Dynamics: {:?}\n\
         - Stereo: {:?}\n\
         - Quality: {:?}\n\n\
         Respond ONLY with valid JSON:\n\
         {{\"narrative\": \"string\", \"action\": null | \
         {{\"type\": \"macro_change|flavour_switch|nothing\", \
         \"handle\": \"tone|dynamics|space|loudness|width\", \
         \"delta\": float, \"reason\": \"string\", \
         \"to\": \"warm|clean|punch|air|film|broadcast\"}}, \
         \"confidence\": float}}",
        schema,
        behaviour.loudness,
        behaviour.spectral,
        behaviour.dynamics,
        behaviour.stereo,
        behaviour.quality,
    )
}

fn parse_action(action: Option<LlmAction>) -> Option<JiniAction> {
    let Some(a) = action else {
        return Some(JiniAction::SuggestNothing);
    };
    match a.kind.as_str() {
        "macro_change" => {
            let handle = match a.handle.as_deref() {
                Some("tone")     => MacroHandle::Tone,
                Some("dynamics") => MacroHandle::Dynamics,
                Some("space")    => MacroHandle::Space,
                Some("loudness") => MacroHandle::Loudness,
                Some("width")    => MacroHandle::Width,
                _ => return Some(JiniAction::SuggestNothing),
            };
            let delta = a.delta.unwrap_or(0.0).clamp(-0.3, 0.3);
            Some(JiniAction::SuggestMacroChange {
                handle,
                delta,
                reason: a.reason.unwrap_or_default(),
            })
        }
        "flavour_switch" => {
            let to = match a.to.as_deref() {
                Some("warm")      => FlavourId::Warm,
                Some("clean")     => FlavourId::Clean,
                Some("punch")     => FlavourId::Punch,
                Some("air")       => FlavourId::Air,
                Some("film")      => FlavourId::Film,
                Some("broadcast") => FlavourId::Broadcast,
                _ => return Some(JiniAction::SuggestNothing),
            };
            Some(JiniAction::SuggestFlavourSwitch {
                to,
                reason: a.reason.unwrap_or_default(),
            })
        }
        _ => Some(JiniAction::SuggestNothing),
    }
}
