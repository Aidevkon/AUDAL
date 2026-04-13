# Aether — Architecture Reference

**Document:** `aether/architecture/aether-architecture.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5

---

## 1. Overview

Aether is the ML enrichment layer. It sits between LineOS (deterministic)
and Apps (user-facing), enriching data with ML inference and normalizing
all output into deterministic feature vectors before it crosses to LineOS.

```
┌──────────────────────────────────────────────────────────┐
│  Apps (Cockpit)                                          │
│  receives CoachNarrative, feature vectors                │
└──────────────────────────┬───────────────────────────────┘
                           │ non-blocking, parallel
                           ▼
┌──────────────────────────────────────────────────────────┐
│  Aether                                                  │
│                                                          │
│  Devices        Adapters       Coaching                  │
│  (ML inference) (translation)  (hints)                   │
│                                                          │
│  adapter-runtime ← only permitted LLM entry point        │
└──────────────────────────┬───────────────────────────────┘
                           │ validate_output()
                           ▼
                  [ADAPTER BOUNDARY]
                           │ feature vectors only
                           ▼
┌──────────────────────────────────────────────────────────┐
│  LineOS (deterministic)                                  │
│  receives only validated, typed feature vectors          │
└──────────────────────────────────────────────────────────┘
```

---

## 2. Subsystems

### 2.1 Devices

Each device is an isolated ML module with one capability.
Devices never import each other. Devices never call adapters directly.

```
aether/devices/
├── denoise/        E4  — deepfilter-rs neural denoising
├── stems/          —   — Demucs/MDX-Net stem separation
├── upscale/        —   — Real-ESRGAN image super-resolution
├── voice/          E1  — Piper neural TTS
├── link2suno/      E7  — Lyria-3 style extraction
└── whisper/        —   — Candle Whisper speech-to-text
```

**Device interface pattern:**

```rust
pub struct DenoiseDevice { model: OrtModel }

impl DenoiseDevice {
    pub fn infer(&self, input: AudioChunk) -> DenoiseOutput {
        // ML inference — may be stochastic
        // Output is raw ML result — NOT yet a feature vector
    }
}
// Output feeds into feature-adapter → Adapter Boundary
```

**Inference runtime selection per device:**

| Device | Runtime | Reason |
|--------|---------|--------|
| denoise | `ort` | ONNX model, widest compatibility |
| stems | `ort` | ONNX model |
| upscale | `ort` | ONNX model |
| voice | Piper native | TTS engine |
| link2suno | adapter-runtime (Lyria-3) | Generative — LLM path |
| whisper | `candle` | Candle Whisper port (pure Rust) |

### 2.2 Adapters

Adapters are the gatekeepers of the Adapter Boundary.

```
aether/adapters/
├── coach-adapter/      QualityMetrics + hints → CoachNarrative
├── metadata-adapter/   Unstructured text → schema-compliant metadata
├── feature-adapter/    ML audio output → feature-vector.schema.json
└── av-adapter/         ML video output → AV feature vectors
```

**Adapter call flow:**

```
1. Receive typed input (never serde_json::Value)
2. build_prompt(input) → String
3. adapter_runtime::llm_client::invoke(prompt) → raw String
4. validate_output(raw) → Result<TypedOutput, AdapterError>
   └── on Err → return AdapterError, write audit event, never pass through
5. Return validated TypedOutput
```

### 2.3 Coaching

Non-blocking contextual intelligence for the Coach Island.

```
aether/coaching/
├── persona-logic/      Selects persona and tone for output
├── hint-generator/     Produces micro-hints (HUD) + structured cards
└── arbitration/        Priority ordering: safety > quality > metadata > learning
```

Coaching runs in parallel with the DSP pipeline. A failed or slow coaching
response never halts mastering. The `PersonaEventBus` decouples coaching
from the Cockpit render cycle.

---

## 3. Adapter Boundary

The firewall between stochastic and deterministic computation.

```
Aether side (stochastic):
  device inference → raw ML output → adapter → validate_output()
                                                      │
                                              ────────┼──── ADAPTER BOUNDARY
                                                      │
LineOS side (deterministic):
  typed feature vector → sp314-dsp / av-core / rule-engine
```

Contract: `creator-os/contracts/feature-vector.schema.json`

**What crosses the boundary:**
- Typed `FeatureVector` structs
- Typed `CoachNarrative` structs
- Validated, schema-compliant JSON

**What never crosses:**
- Raw LLM response strings
- Raw ML inference tensors
- `serde_json::Value`
- Any unvalidated data

---

## 4. Data Flow — Audio Coaching Path

```
sp314-dsp → Golden Blob → QualityMetrics
                                │
                                ▼
              feature-adapter (Aether)
                                │  validate_output()
                                ▼
              feature-vector.schema.json [BOUNDARY]
                                │
                                ▼
              coach-adapter → adapter-runtime → LLM
                                │
                         validate_output()
                                │
                                ▼
              CoachNarrative → PersonaEventBus → Coach Island
```

This path is **parallel and non-blocking**. The DSP pipeline does not wait.

---

## 5. Data Flow — Link2Suno (E7)

```
Golden Blob (audio fingerprint + QualityMetrics)
    │
    ▼
link2suno device
    ├── style_extractor → Lyria-3 (via adapter-runtime) → style features
    ├── lyrics_prompt   → text analysis → lyrics template
    └── suno_prompt     → assembles Suno-compatible prompt
         │
    validate_output()
         │
    [ADAPTER BOUNDARY]
         │
    feature-vector (style + prompt)
         │
    User action → Suno API → returned audio
         │
    M0 → sp314-dsp → Golden Blob (mastered Suno output)
```

**Key:** Link2Suno produces prompts — it does not call Suno directly.
Suno API call is a user-triggered action. The returned audio goes back
through the full LineOS mastering pipeline.

---

## 6. Data Flow — AV Enrichment Path

```
av-core → Golden Blob (AV) → AvQualityMetrics
                                     │
                                     ▼
               av-adapter (Aether)
                                     │  validate_output()
                                     ▼
               AV feature-vector [BOUNDARY]
                                     │
                                     ▼
               coach-adapter → LLM → CoachNarrative
                                     │
               Coach Island (AV-aware hints)
```

---

## 7. Technology Stack

| Component | Technology | Constraint |
|-----------|-----------|-----------|
| Inference (primary) | `ort` ONNX Runtime | All ONNX models |
| Inference (secondary) | `candle` | Rust-native, Whisper |
| Inference (tertiary) | `tract` | Simple models only, confirm ops |
| Audio ML | `deepfilter-rs` | E4 — pure Rust wrapper |
| Stem separation | Demucs ONNX | Via `ort` |
| TTS | Piper | E1 SpeakForge |
| STT | Candle Whisper | NOT whisper-rs (C++) |
| Generative | adapter-runtime | LLM + Lyria-3 — sole entry point |
| Serialization | `serde_json` | Native crates (not WASM boundary) |

**All models are provisioned at install time via `lineos/m0/assets/`.**
No model downloads at runtime. Zero network calls from device inference.

---

## 8. Dependency Direction

```
aether/  →  creator-os/contracts/     (feature vectors, schemas)
aether/  →  lineos/ interfaces         (QualityMetrics input only)
aether/  →  creator-os/shared/adapter-runtime/

aether/  ✗  lineos/ internal modules
aether/  ✗  M0 directly
aether/  ✗  apps/ internals
devices/ ✗  each other
```

---

## 9. CI Gates

| Gate | What it checks |
|------|---------------|
| `aether-boundary-check.sh` | No raw ML output crossing boundary |
| `ml-origin-check.sh` | No pure DSP in aether/ |
| `llm-contract-check.sh` | All LLM calls through adapter-runtime |
| `layer-isolation-check.sh` | No direct LineOS internal imports |
| inline grep | No `serde_json::Value` in adapter I/O |
| inline grep | No `whisper-rs` in aether/ |
| `cargo deny check licenses` | MIT/Apache2 only |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `aether/architecture/aether-architecture.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
