# Creator OS — LLM Adapter Amendment

**Document:** `constitution/amendments/llm-adapter-amendment-v1.0.md`
**Version:** 1.0
**Date:** 2026-03-31
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.2 §08.3
**Inherits:** `creator-os-invariants.md` v1.1
**Referenced by:** Constitution v2.2 §08.3, §09.1, §11.8, §13

---

## Preamble

This amendment defines the full rules governing LLM Adapters in Creator OS.

LLMs are permitted in Creator OS as **stochastic translators only** — never as
compute engines, never as DSP replacements, never as deterministic processing units.

The Adapter Pattern is the only permitted mechanism for LLM interaction.
This amendment is the authoritative specification for that pattern.

If this document conflicts with the Creator OS Constitution, the Constitution wins.
If this document conflicts with `creator-os-invariants.md`, the invariants win.

---

## §A1 — Definition: What an LLM Adapter Is

An LLM Adapter is a bounded interface component that:

1. Accepts **structured input** from an App or Pipeline
2. Translates that input into an LLM prompt
3. Invokes an LLM via `creator-os/shared/adapter-runtime/`
4. Validates the LLM response against a JSON schema
5. Returns **structured, schema-compliant output** to the caller

An LLM Adapter is **not**:
- A compute engine
- A DSP component
- A deterministic processor
- A business logic layer
- A Pipeline orchestrator

An Adapter's job is translation, not computation.

---

## §A2 — Where LLM Adapters Are Permitted

| Layer | Adapters Permitted | Scope |
|-------|--------------------|-------|
| Apps (A1–A6) | ✅ Yes | User-facing stochastic translation only |
| Pipelines (P1–P4) | ✅ Yes | Configuration output only — never data transformation |
| Engines (E1–E7) | ❌ No | Absolute prohibition |
| LineOS | ❌ No | Absolute prohibition |
| Marketplace (MKT1–MKT7) | ❌ No | Absolute prohibition |
| Gallery (G1, SP314) | ❌ No | Absolute prohibition |

**Pipeline scope clarification:** LLM Adapters in Pipelines may only produce
configuration structures (parameters, routing decisions). They must not
transform or modify data flowing through the pipeline.

---

## §A3 — The Adapter Boundary (Non-Negotiable)

The Adapter Boundary is the point where stochastic LLM output is converted to
deterministic structured data. After this boundary, the system must be fully
deterministic.

```
App / Pipeline
      │
      │ structured input (typed, schema-validated)
      ▼
┌─────────────────────────────────────────────┐
│           LLM Adapter                       │
│                                             │
│  1. build_prompt(input)                     │
│  2. adapter_runtime.invoke(prompt)          │
│  3. validate_output(llm_response)  ← BOUNDARY
│  4. return typed output            ← deterministic from here
└─────────────────────────────────────────────┘
      │
      │ validated, schema-compliant output
      ▼
Engine / Pipeline (deterministic domain)
```

**Rule:** Raw LLM output must never cross the Adapter Boundary.
If `validate_output` fails, the Adapter returns a structured error — never
passes raw or partial LLM output downstream.

---

## §A4 — Adapter Runtime (Shared Infrastructure)

All LLM invocation must go through:

```
creator-os/shared/adapter-runtime/
├── llm-client.rs       ← sole permitted LLM invocation point
├── retry.rs            ← retry logic (only permitted retry implementation)
├── validation.rs       ← output schema validation
└── registry.rs         ← adapter registration and discovery
```

**Rules:**

- `llm-client.rs` is the **only** permitted point for LLM API calls in the system.
- No adapter may call an LLM API directly — all calls route through `llm-client.rs`.
- `retry.rs` is the **only** permitted retry implementation. Adapters, Apps, and
  Pipelines must not implement their own retry logic.
- No new file may be added to `adapter-runtime/` without a constitution amendment.
- The adapter-runtime is a shared singleton — no parallel implementation exists.

---

## §A5 — Adapter Implementation Requirements

Every LLM Adapter must:

### §A5.1 — Implement the LLMAdapter Trait

```rust
pub trait LLMAdapter {
    type Input: DeserializeOwned + JsonSchema;
    type Output: Serialize + JsonSchema;

    fn build_prompt(&self, input: &Self::Input) -> String;
    fn validate_output(&self, raw: &str) -> Result<Self::Output, AdapterError>;
    fn adapter_id(&self) -> &'static str;
    fn schema_version(&self) -> &'static str;
}
```

Compile-time enforcement — every adapter must implement this trait.

### §A5.2 — Typed I/O Only

```rust
// ❌ Forbidden — untyped input
fn adapt(input: serde_json::Value) -> serde_json::Value { ... }

// ✅ Required — typed input and output
fn adapt(input: SynthesisRequest) -> Result<SynthesisConfig, AdapterError> { ... }
```

### §A5.3 — Schema Validation Before Return

Every adapter must validate its output against a JSON schema before returning.
Validation failure = `AdapterError::InvalidOutput` — never a partial or raw return.

### §A5.4 — Declared in adapter-registry.json

Every adapter must be declared in its app-level `adapter-registry.json`:

```json
{
  "adapters": [
    {
      "id": "synthesis-config-adapter",
      "version": "1.0",
      "app": "A1",
      "layer": "apps",
      "status": "active",
      "input_schema": "contracts/synthesis-request.schema.json",
      "output_schema": "contracts/synthesis-config.schema.json"
    }
  ]
}
```

An adapter not declared in the registry must not execute at runtime.

---

## §A6 — What Adapters Must Not Do

```
❌ Call LLM APIs directly (bypassing adapter-runtime)
❌ Pass raw LLM output to any Engine
❌ Implement retry logic (use adapter-runtime/retry.rs)
❌ Accept serde_json::Value as input
❌ Return serde_json::Value as output
❌ Perform DSP, audio processing, or data transformation
❌ Hold state between calls
❌ Access filesystem, network, or process operations directly
❌ Execute inside Engines (E1–E7), LineOS, Marketplace, or Gallery
❌ Run without a registry entry with status: "active"
❌ Produce output that enters a deterministic Engine without validate_output passing
```

---

## §A7 — Determinism Contract

LLMs are stochastic by nature. This is acceptable only within the Adapter boundary.

- After `validate_output`, all downstream processing must be deterministic.
- LLM output must never influence a deterministic Engine computation directly.
- Adapter output, once validated and typed, is treated as deterministic input.
- An Adapter that produces different output for identical inputs is expected behavior
  and is not a violation — stochasticity is contained within the Adapter boundary.
- An Engine that produces different output for identical validated inputs is always
  a violation — Engines are deterministic domains.

---

## §A8 — CI Enforcement Gates

All CI gates are mandatory on every commit touching adapter code.

| Gate | Check | Failure |
|------|-------|---------|
| No direct LLM calls | `grep -r 'reqwest\|openai\|anthropic' --include="*.rs" \| grep -v adapter-runtime` → 0 results | Build failure |
| Trait implementation | All adapters implement `LLMAdapter` trait | Compile failure |
| No adapters in Engines | `grep -r 'LLMAdapter\|adapter_runtime' engines/` → 0 results | Build failure |
| No adapters in LineOS | `grep -r 'LLMAdapter\|adapter_runtime' lineos/` → 0 results | Build failure |
| Registry completeness | `adapter-registry.json` validates against registry schema, all IDs unique | Build failure |
| No `serde_json::Value` in adapter I/O | `grep -r 'serde_json::Value' adapters/` → 0 results in public fn signatures | Build failure |
| Output validation required | Static analysis: every `invoke()` call followed by `validate_output()` | Build failure |

---

## §A9 — Forbidden Patterns (Quick Reference)

```rust
// ❌ Direct LLM call — forbidden
let response = openai::complete(prompt).await?;

// ❌ Raw output to Engine — forbidden
engine.process(llm_response.raw_text);

// ❌ serde_json::Value I/O — forbidden
fn adapt(input: serde_json::Value) -> serde_json::Value

// ❌ Retry in Adapter — forbidden
for attempt in 0..3 { ... }

// ❌ Adapter in Engine — forbidden
// Any LLMAdapter impl inside engines/ crate

// ✅ Correct pattern
let raw = adapter_runtime::llm_client::invoke(&prompt).await?;
let output: SynthesisConfig = self.validate_output(&raw)?;
Ok(output)
```

---

## §A10 — Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-31 | Initial amendment — extracted from Constitution v2.2 §08.3, §09.1, §11.8, §13 |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `constitution/amendments/llm-adapter-amendment-v1.0.md`
**Version:** 1.0
**Date:** 2026-03-31
**Status:** 🔒 LOCKED

---

*Adapters translate. Engines compute. The boundary between them is law.*
