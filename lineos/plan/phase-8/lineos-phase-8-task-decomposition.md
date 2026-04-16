# LineOS — Phase 8 Task Decomposition

**Document:** `lineos/plan/phase-8/task-decomposition.md`
**Version:** 1.0
**Phase:** 8 — Aether Coach (LLM Narrative)
**Status:** 🔒 LOCKED
**Authority:** Phase 8 Master Prompt · LLM Adapter Amendment v1.1

---

## Architecture

```
apps/stillair/src-tauri/
├── src/
│   ├── commands/
│   │   └── coach.rs          ← NEW — Tauri command: get_coach_narrative
│   └── aether/               ← NEW — Aether layer in Tauri process
│       ├── mod.rs
│       ├── coach_adapter.rs  ← LLMAdapter impl
│       └── adapter_registry.json

creator-os/shared/adapter-runtime/
├── src/
│   ├── llm-client.rs         ← ADD Ollama provider
│   └── providers/
│       └── ollama.rs         ← NEW
```

**Layer rule:** Aether code lives in `src/aether/` inside the Tauri
process. It is NOT in the WASM frontend. The frontend receives only
the validated `CoachNarrativeJson` struct via Tauri IPC.

---

## Task Order

```
P8-001  Install Ollama + pull models
P8-002  Ollama provider in adapter-runtime
P8-003  CoachNarrativeJson type (IPC contract)
P8-004  CoachAdapter — LLMAdapter trait impl
P8-005  Tauri command: get_coach_narrative
P8-006  Wire Right MFD — display CoachNarrative
P8-007  adapter-registry.json
P8-008  A/B provider config (phi vs gemma)
P8-009  CI gate + tag
```

---

## P8-001 — Install Ollama + Pull Models

```bash
# Install Ollama if not present
curl -fsSL https://ollama.ai/install.sh | sh

# Pull models
ollama pull phi3.5:3.8b          # ~2.2GB — structured JSON
ollama pull gemma2:9b       # ~5.5GB — narrative quality

# Verify
ollama list
curl -s http://localhost:11434/api/tags | python3 -m json.tool
```

**DoD P8-001:**
```bash
ollama list | grep -E "phi3.5:3.8b|gemma2:9b"
echo "✅ P8-001"
```

---

## P8-002 — Ollama Provider in adapter-runtime

Add `creator-os/shared/adapter-runtime/src/providers/ollama.rs`:

```rust
//! Ollama local inference provider.
//! Used for dev/test: phi3.5:3.8b (JSON), gemma2:9b (narrative).
//! Production: swap to Lyria 3 provider without code changes.

use serde::{Deserialize, Serialize};

const OLLAMA_BASE: &str = "http://localhost:11434";

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model:  &'a str,
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

/// Call Ollama with a prompt. Returns raw response string.
/// Always use via adapter-runtime — never call directly from adapters.
pub async fn invoke(model: &str, prompt: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let req = OllamaRequest {
        model,
        prompt,
        stream: false,
        options: OllamaOptions {
            temperature: 0.3,   // low temp → consistent output
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
```

Add to `llm-client.rs`:
```rust
pub mod ollama;

pub enum Provider {
    Ollama { model: String },
    // Claude, Lyria — future
}

pub async fn invoke(provider: &Provider, prompt: &str) -> Result<String, String> {
    match provider {
        Provider::Ollama { model } => ollama::invoke(model, prompt).await,
    }
}
```

Add `reqwest` to adapter-runtime Cargo.toml if not present.

**DoD P8-002:**
```bash
cargo check -p adapter-runtime
echo "✅ P8-002"
```

---

## P8-003 — CoachNarrativeJson Type

**IPC contract** — what the frontend receives.
No raw LLM output ever crosses this boundary.

```rust
// apps/stillair/src-tauri/src/aether/mod.rs

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CoachNarrativeJson {
    pub summary:      String,
    pub explanations: Vec<FindingExplanation>,
    pub model_used:   String,   // "phi3.5:3.8b" or "gemma2:9b"
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct FindingExplanation {
    pub issue_id:   String,
    pub severity:   String,
    pub title:      String,
    pub why:        String,   // why this matters
    pub suggestion: String,   // what to try (direction only, no DSP values)
}
```

Mirror in `frontend/src/types.rs` for WASM deserialization.

**DoD P8-003:**
```bash
cargo check -p stillair
cargo check --target wasm32-unknown-unknown -p stillair-frontend
echo "✅ P8-003"
```

---

## P8-004 — CoachAdapter

```rust
// apps/stillair/src-tauri/src/aether/coach_adapter.rs

use crate::commands::insights::CoachFindingsJson;
use super::{CoachNarrativeJson, FindingExplanation};
use adapter_runtime::llm_client::{invoke, Provider};

pub struct CoachAdapter {
    pub provider: Provider,
}

impl CoachAdapter {
    pub fn phi() -> Self {
        Self { provider: Provider::Ollama { model: "phi3.5:3.8b".into() } }
    }

    pub fn gemma() -> Self {
        Self { provider: Provider::Ollama { model: "gemma2:9b".into() } }
    }

    pub async fn generate(
        &self,
        findings: &CoachFindingsJson,
    ) -> Result<CoachNarrativeJson, String> {
        let prompt = self.build_prompt(findings);
        let raw = invoke(&self.provider, &prompt).await?;
        self.validate_output(&raw, findings)
    }

    fn build_prompt(&self, findings: &CoachFindingsJson) -> String {
        let issues_text = findings.issues.iter().map(|i| {
            format!(
                "- {} [{}]: current={:.1}, target={:.1}, delta={:.1}",
                i.id, i.severity, i.current, i.target, i.delta
            )
        }).collect::<Vec<_>>().join("\n");

        format!(
            r#"You are a professional audio mastering coach. Your role is to explain
audio analysis findings in plain language. You are a teacher, not an engineer.
Never give specific DSP values or plugin instructions.
Explain WHY each finding matters and give directional suggestions only.

Audio analysis findings:
{issues_text}

Overall recommendation: {recommendation}

Respond ONLY with valid JSON matching this exact schema:
{{
  "summary": "2-3 sentence overview",
  "explanations": [
    {{
      "issue_id": "exact_issue_id",
      "severity": "info|low|medium|high",
      "title": "short title",
      "why": "why this matters for the listener",
      "suggestion": "what direction to explore (no specific values)"
    }}
  ]
}}

JSON only. No markdown. No preamble."#,
            issues_text = issues_text,
            recommendation = findings.recommendation,
        )
    }

    fn validate_output(
        &self,
        raw: &str,
        findings: &CoachFindingsJson,
    ) -> Result<CoachNarrativeJson, String> {
        // Strip any markdown fences
        let clean = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        #[derive(serde::Deserialize)]
        struct LlmOutput {
            summary:      String,
            explanations: Vec<FindingExplanation>,
        }

        let parsed: LlmOutput = serde_json::from_str(clean)
            .map_err(|e| format!("CoachAdapter: invalid JSON from LLM: {e}\nRaw: {clean}"))?;

        // Validate: every explanation must reference a known issue_id
        let known_ids: std::collections::HashSet<&str> =
            findings.issues.iter().map(|i| i.id.as_str()).collect();

        for exp in &parsed.explanations {
            if !known_ids.contains(exp.issue_id.as_str()) {
                return Err(format!(
                    "CoachAdapter: unknown issue_id '{}' in LLM output",
                    exp.issue_id
                ));
            }
        }

        let model_name = match &self.provider {
            adapter_runtime::llm_client::Provider::Ollama { model } => model.clone(),
        };

        Ok(CoachNarrativeJson {
            summary:      parsed.summary,
            explanations: parsed.explanations,
            model_used:   model_name,
        })
    }
}
```

**DoD P8-004:**
```bash
cargo check -p stillair
echo "✅ P8-004"
```

---

## P8-005 — Tauri Command: get_coach_narrative

```rust
// apps/stillair/src-tauri/src/commands/coach.rs

use crate::aether::coach_adapter::CoachAdapter;
use crate::commands::insights::CoachFindingsJson;
use crate::aether::CoachNarrativeJson;

/// Provider selection via env var for A/B testing:
/// COACH_PROVIDER=phi (default) or COACH_PROVIDER=gemma
#[tauri::command]
pub async fn get_coach_narrative(
    findings: CoachFindingsJson,
) -> Result<CoachNarrativeJson, String> {
    let provider = std::env::var("COACH_PROVIDER")
        .unwrap_or_else(|_| "phi".into());

    let adapter = match provider.as_str() {
        "gemma" => CoachAdapter::gemma(),
        _       => CoachAdapter::phi(),
    };

    adapter.generate(&findings).await
        .map_err(|e| format!("IO_ERR:0x02:Coach narrative failed: {e}"))
}
```

Register in `lib.rs`:
```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    commands::coach::get_coach_narrative,
])
```

**DoD P8-005:**
```bash
cargo check -p stillair
echo "✅ P8-005"
```

---

## P8-006 — Wire Right MFD

Update `frontend/src/cockpit/coach_panel.rs`:

After `evaluate_findings` returns `CoachFindings`, immediately invoke
`getCoachNarrative` and display the result.

```rust
// In app.rs, after coach_done() transition (FM5):
let narrative_result = invoke::<CoachNarrativeJson>(
    "getCoachNarrative",
    serde_json::json!({ "findings": findings }),
).await;

match narrative_result {
    Ok(narrative) => set_narrative.set(Some(narrative)),
    Err(e) => {
        // Non-fatal: show findings without narrative
        leptos::logging::warn!("Coach narrative failed: {e}");
        set_narrative.set(None);
    }
}
```

Right MFD display (FM5):
```
┌─ THE COACH ──────────────────────────────┐
│                                          │
│ SUMMARY                                  │
│ [narrative.summary]                      │
│                                          │
│ ─────────────────────────────────────── │
│                                          │
│ ● LUFS COMPLIANCE  [Medium]              │
│   [explanation.why]                      │
│   → [explanation.suggestion]             │
│                                          │
│ ● TRUE PEAK  [Low]                       │
│   [explanation.why]                      │
│   → [explanation.suggestion]             │
│                                          │
│ model: phi3.5:3.8b                            │
└──────────────────────────────────────────┘
```

**DoD P8-006:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-frontend
echo "✅ P8-006"
```

---

## P8-007 — adapter-registry.json

Create `apps/stillair/src-tauri/adapter-registry.json`:

```json
{
  "adapters": [
    {
      "id": "coach-narrative-adapter",
      "version": "1.0",
      "app": "A1",
      "layer": "aether",
      "status": "active",
      "provider": "ollama",
      "models": ["phi3.5:3.8b", "gemma2:9b"],
      "input_schema": "contracts/coach-findings.schema.json",
      "output_schema": "contracts/coach-narrative.schema.json"
    }
  ]
}
```

**DoD P8-007:**
```bash
python3 -c "import json; json.load(open('apps/stillair/src-tauri/adapter-registry.json')); print('✅ valid JSON')"
echo "✅ P8-007"
```

---

## P8-008 — A/B Provider Config

A/B switching via environment variable — zero code changes:

```bash
# Default (phi3.5:3.8b — structured JSON)
cargo tauri dev

# Switch to gemma2:9b — narrative quality
COACH_PROVIDER=gemma cargo tauri dev
```

Add to `.env.example` in repo root:
```
# Coach LLM provider: phi (default) or gemma
COACH_PROVIDER=phi
```

**DoD P8-008:**
```bash
# Verify env var is read
grep -n "COACH_PROVIDER" apps/stillair/src-tauri/src/commands/coach.rs
echo "✅ P8-008"
```

---

## P8-009 — CI Gate + Tag

```bash
cargo test --workspace
just ci
just deny

git add -A
git commit -m "feat(coach): Phase 8 — Aether Coach LLM narrative

- Ollama provider in adapter-runtime (phi3.5:3.8b + gemma2:9b)
- CoachAdapter: LLMAdapter impl with validate_output()
- Prompt: teacher identity, no DSP instructions
- validate_output: JSON schema + issue_id cross-validation
- get_coach_narrative Tauri command
- Right MFD: summary + per-finding explanations
- A/B switching via COACH_PROVIDER env var
- adapter-registry.json: coach-narrative-adapter v1.0

Architecture enforced:
  - LLM only via adapter-runtime (never direct)
  - Raw LLM output never crosses Adapter Boundary
  - Coach never modifies CoachFindings
  - Frontend receives typed CoachNarrativeJson only

Authority: LLM Adapter Amendment v1.1 · Creator OS Constitution v2.6"

git tag v0.8.0-coach
git log --oneline -5
```

---

## Completion Report

```
✅ Phase 8 — Aether Coach — COMPLETE

Ollama provider:    phi3.5:3.8b + gemma2:9b ✅
CoachAdapter:       validate_output() enforced ✅
Adapter Boundary:   raw LLM never crosses ✅
Right MFD:          real narrative displayed ✅
A/B switching:      COACH_PROVIDER env var ✅
just ci:            ✅

Tag: v0.8.0-coach ✅

Coach identity: Teacher. Never engineer.
Ready for: Phase 9 — Export
```

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1) — Aether Layer
**Phase:** 8
**Version:** 1.0
**Status:** 🔒 LOCKED
