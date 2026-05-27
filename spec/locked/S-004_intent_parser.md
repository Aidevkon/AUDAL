# S-004 — Intent Parser (LLM + Rule)

**Document:** `spec/locked/S-004_intent_parser.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Aether
**Depends on:** S-003 (Persona Schema)
**Used by:** S-005 (Macro → Micro Mapping)
**Audit:** DeepSeek v0.1 → REJECT · v0.2 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | Promoted to LOCKED. R1: validate_against_persona note added. R2: Clone note added. |
| 0.2 | 2026-05-27 | Critical: §6 Resolution Order. C2–C5 addressed. +2 tests. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Intent Parser converts user input — either natural language text
(via LLM) or UI macro handle values (rule-based) — into a structured,
validated `Intent` object.

The Intent object is the single entry point for all user creative
decisions into the Aether pipeline. It is always validated against
`contracts/intent.schema.json` before crossing to S-005.

**One sentence:** Given user input (text or UI handles), produce a
validated, typed Intent that drives the macro mapping layer.

---

## 2. Constitutional Position

```
User input (text | UI handles)
    ↓
IntentParser (S-004)
    │
    ├── Rule path (UI handles) → deterministic, no LLM
    └── LLM path (text)       → adapter-runtime only
                                → validate_output()
                                → [ADAPTER BOUNDARY]
    ↓
Intent (typed, validated)
    ↓ §6 Resolution Order applied
    ↓ validate against intent.schema.json
    ↓
S-005 (Macro → Micro Mapping)
```

**Rules:**
- LLM path: adapter-runtime ONLY — no direct LLM API calls
- Rule path: pure deterministic function — no ML, no randomness
- LLM output is suggestion only — validated before use
- Intent JSON validated against `contracts/intent.schema.json`
- If LLM unavailable → fallback preserves current macro values
- Intent never contains DSP parameters directly — macros only
- Intent macros are ABSOLUTE values — never relative/delta
- Delta adjustments belong exclusively in S-008 (Auto-Tuning)

---

## 3. Interface

### Constants

```rust
pub const INTENT_MACRO_MIN:      f32   = 0.0;
pub const INTENT_MACRO_MAX:      f32   = 1.0;
pub const INTENT_CONFIDENCE_MIN: f32   = 0.0;
pub const INTENT_CONFIDENCE_MAX: f32   = 1.0;
pub const INTENT_LLM_TIMEOUT_MS: u64   = 5_000;
pub const INTENT_MAX_TEXT_LEN:   usize = 500;
```

### Intent (output)

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Intent {
    /// Source of this intent
    pub source: IntentSource,

    /// Resolved persona ID (from S-003)
    /// None = keep current persona
    pub persona_id: Option<String>,

    /// Macro handle values — ABSOLUTE [0.0, 1.0]
    /// None = keep current value (no change)
    /// Delta adjustments are forbidden here — see S-008
    pub macros: IntentMacros,

    /// Confidence [0.0, 1.0] — informational only, never used for decisions
    /// Rule path = 1.0. LLM path = model confidence.
    /// confidence=0.0 signals LLM fallback to current values.
    pub confidence: f32,

    /// Human-readable explanation (Tier 2/3 UI disclosure)
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IntentSource {
    UiHandles,
    LlmText,
    Default,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IntentMacros {
    pub warmth:      Option<f32>,  // [0.0, 1.0] or None=no change
    pub punch:       Option<f32>,
    pub forwardness: Option<f32>,
    pub smoothness:  Option<f32>,
}
```

### IntentParser

```rust
pub struct IntentParser;

impl IntentParser {
    /// Rule path: map UI handles directly to Intent.
    /// Deterministic — confidence always 1.0.
    pub fn from_ui_handles(handles: &MacroControls) -> Intent;

    /// LLM path: parse natural language text.
    /// current_macros: active session macro values (for fallback).
    /// Falls back to current_macros on timeout/error (confidence=0.0).
    /// Returns Err only on schema validation failure.
    pub async fn from_text(
        text:           &str,
        persona:        &PersonaConfig,
        current_macros: &MacroControls,  // from active session state
    ) -> Result<Intent, IntentError>;

    /// Apply §6 resolution order.
    /// Assumes persona_id is already validated by resolve_persona().
    /// Returns cloned PersonaConfig (PersonaConfig derives Clone).
    pub fn resolve(
        intent:          &Intent,
        current_persona: &PersonaConfig,
        current_macros:  &MacroControls,
    ) -> (PersonaConfig, MacroControls);
}

#[derive(Debug)]
pub enum IntentError {
    TextTooLong(usize),
    SchemaValidationFailed(String),
    LlmAdapterError(String),
}
```

---

## 4. Rule Path — UI Handles

```rust
pub fn from_ui_handles(handles: &MacroControls) -> Intent {
    Intent {
        source:     IntentSource::UiHandles,
        persona_id: None,
        macros: IntentMacros {
            warmth:      Some(handles.warmth
                              .clamp(INTENT_MACRO_MIN, INTENT_MACRO_MAX)),
            punch:       Some(handles.punch
                              .clamp(INTENT_MACRO_MIN, INTENT_MACRO_MAX)),
            forwardness: Some(handles.forwardness
                              .clamp(INTENT_MACRO_MIN, INTENT_MACRO_MAX)),
            smoothness:  Some(handles.smoothness
                              .clamp(INTENT_MACRO_MIN, INTENT_MACRO_MAX)),
        },
        confidence:  1.0,
        explanation: None,
    }
}
```

---

## 5. LLM Path — Text Parsing

### Prompt Template (versioned)

```rust
pub const PROMPT_TEMPLATE_VERSION: &str = "1.0";

pub const PROMPT_TEMPLATE: &str = r#"
You are an audio mastering assistant.
Available controls:
  warmth      [0.0–1.0]: low-end fullness and tape saturation
  punch       [0.0–1.0]: transient attack and dynamic impact
  forwardness [0.0–1.0]: vocal/mid presence in the mix
  smoothness  [0.0–1.0]: high-frequency softness and release

Current persona: {persona_name}
Current values (from active session):
  warmth={w}, punch={p}, forwardness={f}, smoothness={s}

User request: "{user_text}"

Respond ONLY with valid JSON:
{
  "persona_id":   string | null,
  "warmth":       number | null,
  "punch":        number | null,
  "forwardness":  number | null,
  "smoothness":   number | null,
  "confidence":   number,
  "explanation":  string
}
Rules:
- Only set values explicitly requested
- Values must be in [0.0, 1.0]
- null = no change
- Absolute values only — no deltas
"#;
```

### Fallback Strategy

```
LLM timeout (>INTENT_LLM_TIMEOUT_MS):
    → log warning
    → return Intent with all macros=None (preserve current)
    → confidence = 0.0

LLM unavailable / no API key:
    → skip LLM silently
    → return Intent with all macros=None (preserve current)
    → confidence = 0.0

LLM schema validation fail:
    → return Err(SchemaValidationFailed)
    → caller falls back to current macros

Fallback always preserves current_macros — never resets to defaults.
```

---

## 6. Persona & Macro Resolution Order ← AUTHORITATIVE

This section defines the exact, unambiguous resolution order.

```rust
/// Resolution order:
///
/// Step 1: Persona switch
///   intent.persona_id = Some(id) → PersonaManager::get(id).clone()
///   intent.persona_id = None     → current_persona.clone()
///
/// Step 2: Macro application (ABSOLUTE)
///   For each macro field:
///     Some(value) → set to that absolute value
///     None        → keep current_macros value
///
///   Macros are ALWAYS absolute — never relative to persona defaults.
///   Example: LLM says warmth=0.8 after switching to clean_punch
///   → warmth=0.8 (absolute), NOT clean_punch.default.warmth + 0.8
///
/// Step 3: Clamp to (new) persona handle bounds
///   Each value clamped to persona.macros.X.[min, max]
///   Example: clean_punch.warmth.max=0.7 → warmth=0.8 clamped to 0.7
///
/// Step 4: Output → S-005 (Macro → Micro Mapping)

pub fn resolve(
    intent:          &Intent,
    current_persona: &PersonaConfig,
    current_macros:  &MacroControls,
) -> (PersonaConfig, MacroControls) {
    // Step 1
    let persona = intent.persona_id
        .as_deref()
        .and_then(|id| PersonaManager::get(id))
        .unwrap_or_else(|| current_persona.clone());

    // Step 2
    let macros = MacroControls {
        warmth:      intent.macros.warmth
                         .unwrap_or(current_macros.warmth),
        punch:       intent.macros.punch
                         .unwrap_or(current_macros.punch),
        forwardness: intent.macros.forwardness
                         .unwrap_or(current_macros.forwardness),
        smoothness:  intent.macros.smoothness
                         .unwrap_or(current_macros.smoothness),
    };

    // Step 3
    let macros = MacroControls {
        warmth:      macros.warmth
                         .clamp(persona.macros.warmth.min,
                                persona.macros.warmth.max),
        punch:       macros.punch
                         .clamp(persona.macros.punch.min,
                                persona.macros.punch.max),
        forwardness: macros.forwardness
                         .clamp(persona.macros.forwardness.min,
                                persona.macros.forwardness.max),
        smoothness:  macros.smoothness
                         .clamp(persona.macros.smoothness.min,
                                persona.macros.smoothness.max),
    };

    (persona, macros)
}
```

---

## 7. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Rule path | ✅ Pure function — same input → same output |
| LLM path | ⚠️ Stochastic — contained by Adapter Boundary |
| LLM fallback | ✅ Preserves current macros — deterministic |
| Resolution order | ✅ Explicit, unambiguous (§6) |
| Macro values | ✅ Always absolute — no delta ambiguity |
| Confidence | ✅ Informational only — never affects decisions |

---

## 8. Performance Targets

| Metric | Target |
|--------|--------|
| Rule path | < 100μs |
| LLM path | < 5s (timeout) |
| Fallback | < 100μs |

---

## 9. Contract Tests

```rust
#[test]
fn intent_from_ui_handles_deterministic() {
    let h = MacroControls::default();
    assert_eq!(
        IntentParser::from_ui_handles(&h),
        IntentParser::from_ui_handles(&h)
    );
}

#[test]
fn intent_from_ui_handles_confidence_one() {
    let i = IntentParser::from_ui_handles(&MacroControls::default());
    assert_eq!(i.confidence, 1.0);
    assert_eq!(i.source, IntentSource::UiHandles);
}

#[test]
fn intent_macro_values_clamped() {
    let h = MacroControls { warmth:2.0, punch:-1.0,
                            forwardness:0.5, smoothness:0.5 };
    let i = IntentParser::from_ui_handles(&h);
    assert_eq!(i.macros.warmth, Some(1.0));
    assert_eq!(i.macros.punch,  Some(0.0));
}

#[test]
fn intent_validates_schema() {
    let i = IntentParser::from_ui_handles(&MacroControls::default());
    let json = serde_json::to_string(&i).unwrap();
    assert!(validate_against_schema(&json, "intent.schema.json"));
}

#[test]
fn intent_persona_unknown_resolves_to_none() {
    let result = resolve_persona(Some("nonexistent"));
    assert_eq!(result, None);
}

#[test]
fn intent_persona_valid_resolves() {
    let result = resolve_persona(Some("warm_analog"));
    assert_eq!(result, Some("warm_analog".to_string()));
}

#[test]
fn intent_resolution_persona_switch_with_absolute_macros() {
    // clean_punch warmth max = 0.7
    // Intent: switch to clean_punch AND set warmth=0.9 (absolute)
    // Expected: warmth clamped to 0.7
    let intent = Intent {
        source:     IntentSource::LlmText,
        persona_id: Some("clean_punch".to_string()),
        macros:     IntentMacros {
            warmth: Some(0.9), punch: None,
            forwardness: None, smoothness: None,
        },
        confidence: 0.9, explanation: None,
    };
    let (new_persona, new_macros) = IntentParser::resolve(
        &intent,
        &PersonaManager::get("warm_analog").unwrap(),
        &MacroControls::default(),
    );
    assert_eq!(new_persona.id, "clean_punch");
    assert!(new_macros.warmth <= 0.7,
        "warmth should be clamped: {}", new_macros.warmth);
}

#[test]
fn intent_resolution_none_macro_preserves_current() {
    let intent = Intent {
        source:     IntentSource::LlmText,
        persona_id: None,
        macros:     IntentMacros {
            warmth: None, punch: Some(0.8),
            forwardness: None, smoothness: None,
        },
        confidence: 1.0, explanation: None,
    };
    let current = MacroControls {
        warmth:0.3, punch:0.5, forwardness:0.6, smoothness:0.7
    };
    let (_, new_macros) = IntentParser::resolve(
        &intent,
        &PersonaManager::get("warm_analog").unwrap(),
        &current,
    );
    assert_eq!(new_macros.warmth,      0.3);  // unchanged
    assert!((new_macros.punch - 0.8).abs() < 1e-6);
    assert_eq!(new_macros.forwardness, 0.6);  // unchanged
    assert_eq!(new_macros.smoothness,  0.7);  // unchanged
}
```

---

## 10. Error Handling

| Condition | Behavior |
|-----------|----------|
| Text too long | `Err(TextTooLong)` |
| LLM timeout | Preserve current macros, confidence=0.0 |
| LLM unavailable | Skip silently, preserve current macros |
| Schema validation fail | `Err(SchemaValidationFailed)` |
| Unknown persona in LLM output | `persona_id=None`, continue |
| NaN/Inf in LLM output | Clamp to [0.0,1.0], log warning |

---

## 11. Implementation Path

```
aether/intent/
├── mod.rs       ← pub use parser::IntentParser
├── types.rs     ← Intent, IntentSource, IntentMacros, IntentError, constants
├── parser.rs    ← IntentParser impl (rule path)
├── prompt.rs    ← PROMPT_TEMPLATE (versioned)
├── resolver.rs  ← resolve(), resolve_persona()
└── validator.rs ← validate_output() (LLM path)
```

**LLM calls via:**
`creator-os/shared/adapter-runtime/llm-client.rs`

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. R1: resolve note added. R2: Clone note added. |
| 0.2 | 2026-05-27 | Critical §6 + C2–C5 + 2 tests. |
| 0.1 | 2026-05-27 | Initial draft |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-004_intent_parser.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*Rule path: deterministic. LLM path: stochastic, firewalled.*
*Resolution order: §6. Always.*
