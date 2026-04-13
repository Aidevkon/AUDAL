# Aether — Constitution

**Document:** `aether/constitution/aether-constitution.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.5 — ML enrichment layer
**Inherits:** `creator-os/invariants/creator-os-invariants.md` v1.1

---

## Preamble

This document is the constitution of Aether.

Aether is the ML enrichment layer of Creator OS. It is the only layer
where stochastic computation is permitted. Its single responsibility is
to enrich deterministic data with ML inference — and then normalize that
inference into deterministic feature vectors before it crosses into LineOS.

Aether imagines. LineOS executes. The Adapter Boundary is the firewall
between them.

If any rule in this document conflicts with the Creator OS Constitution,
the Creator OS Constitution wins. If it conflicts with `creator-os-invariants.md`,
the invariants win.

---

## §01 — What Aether Is

Aether is the **ML enrichment layer** of Creator OS. It provides:

- **Devices** — isolated ML modules, one capability each
- **Adapters** — translate ML/LLM output into schema-compliant feature vectors
- **Coaching** — contextual intelligence for the Cockpit Coach Island
- **adapter-runtime** — the only permitted entry point for LLM and Lyria calls

**One sentence:** Aether enriches with ML and normalises all output into
deterministic vectors — it never executes, never persists, never decides.

### §01.1 — What Aether Is Not

- ❌ A DSP layer — Aether never processes audio or video directly
- ❌ A compute substrate — LineOS executes; Aether enriches
- ❌ A decision engine — Aether gives hints, never makes decisions
- ❌ A persistent store — Aether holds no state between calls
- ❌ A UI layer — Aether never writes to OLED islands or generates geometry
- ❌ A deterministic system — Aether is explicitly stochastic, contained by the Adapter Boundary

### §01.2 — Relationship to LineOS

Aether depends on:
- `lineos/` interfaces (read-only — receives QualityMetrics, AV metrics)
- `creator-os/contracts/` (feature vectors, coach narratives, schemas)

Aether **never imports** LineOS internal modules.
Aether **never calls** M0 directly.
Aether communicates with LineOS only through validated feature vectors
crossing the Adapter Boundary.

---

## §02 — The Adapter Boundary (Non-Negotiable)

The Adapter Boundary is the most important concept in Aether.

```
Aether (stochastic)
    │
    │  validate_output() ← JSON schema validation
    │  against feature-vector.schema.json
    │
    ▼
[ADAPTER BOUNDARY]
    │
    │  typed, schema-compliant feature vectors only
    │
    ▼
LineOS (deterministic)
```

**Rules — all absolute:**
- Raw ML or LLM output must never cross this boundary
- `validate_output()` must be called on every adapter before returning
- After the boundary, all data is treated as deterministic input
- An adapter that returns unvalidated output is a constitutional violation
- The boundary contract is `creator-os/contracts/feature-vector.schema.json`

---

## §03 — Aether Subsystems

| Subsystem | Path | Responsibility |
|-----------|------|---------------|
| Devices | `aether/devices/` | Isolated ML inference modules |
| Adapters | `aether/adapters/` | ML → validated feature vectors |
| Coaching | `aether/coaching/` | Contextual hints for Cockpit |
| adapter-runtime | `creator-os/shared/adapter-runtime/` | LLM + Lyria gateway |

---

## §04 — Devices

Devices are isolated ML modules. Each device has exactly one capability.
Devices do not import each other. Devices do not call adapters directly.

### §04.1 — Device Rules

- One device = one capability. No multi-purpose devices.
- Devices are stateless — no persistent state between calls
- Device output must be normalized before leaving the device boundary
- Devices may use ONNX models (via `ort` or `candle`) or pure Rust ML
- All models are loaded from `lineos/m0/assets/` — never downloaded at runtime
- Device inference may be stochastic — this is expected and acceptable
- All device output passes through an adapter before crossing to LineOS

### §04.2 — Device Registry

| Device | Path | Engine ID | Model | Purpose |
|--------|------|-----------|-------|---------|
| Denoise | `aether/devices/denoise/` | E4 | deepfilter-rs | Neural audio denoising |
| Stems | `aether/devices/stems/` | — | Demucs / MDX-Net (ONNX) | Stem separation |
| Upscale | `aether/devices/upscale/` | — | Real-ESRGAN (ONNX) | Image super-resolution |
| Voice | `aether/devices/voice/` | E1 | Piper | Neural TTS |
| Link2Suno | `aether/devices/link2suno/` | E7 | Lyria-3 (via adapter-runtime) | Style extraction |
| Whisper | `aether/devices/whisper/` | — | Candle Whisper (pure Rust) | Speech-to-text |

**Whisper binding rule:** Candle Whisper (pure Rust + ONNX) only.
`whisper-rs` and `whisper.cpp` are forbidden — C++ FFI violates the pure Rust constraint.

### §04.3 — ML Inference Priority

When multiple inference runtimes are available:

```
ort (ONNX Runtime)  ← primary — widest model compatibility
    ↓ fallback if model op not supported
candle              ← secondary — Rust-native
    ↓ fallback for simple models only
tract               ← tertiary — pure Rust, limited op support
```

`tract` may only be used for models with confirmed op compatibility.
The inference priority is per-device — document the choice in the device's `README.md`.

---

## §05 — Adapters

Adapters are the gatekeepers of the Adapter Boundary. They translate
ML/LLM output into typed, schema-validated data.

### §05.1 — Adapter Rules

- Every adapter implements the `LLMAdapter` trait (compile-time enforcement)
- Input is always typed — never `serde_json::Value`
- Output is always typed — never `serde_json::Value`
- `validate_output()` must be called before returning any output
- Adapters call LLMs only via `adapter-runtime/llm-client.rs`
- Adapters implement no retry logic — `adapter-runtime/retry.rs` handles retries
- Adapters hold no state between calls — fully stateless
- Adapters execute only in Aether — never in LineOS, engines, or apps

### §05.2 — Adapter Registry

| Adapter | Path | Input | Output |
|---------|------|-------|--------|
| coach-adapter | `aether/adapters/coach-adapter/` | QualityMetrics | CoachNarrative |
| metadata-adapter | `aether/adapters/metadata-adapter/` | Unstructured text | Schema-compliant metadata |
| feature-adapter | `aether/adapters/feature-adapter/` | ML audio output | `feature-vector.schema.json` |
| av-adapter | `aether/adapters/av-adapter/` | ML video output | AV feature vectors |

### §05.3 — LLMAdapter Trait (Required)

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

Full specification: `creator-os/constitution/amendments/llm-adapter-amendment-v1.1.md`

---

## §06 — Coaching

The coaching subsystem produces contextual intelligence for the Cockpit
Coach Island. It operates in parallel with the deterministic pipeline —
never blocking it.

### §06.1 — Coaching Rules

- Coaching is non-blocking — a slow or failed coaching response never halts the pipeline
- Coaching gives hints — it does not make decisions
- Coaching output routes through `PersonaEventBus` to the Coach Island
- Coaching is not to be confused with `lineos/m1/rule-engine/` which evaluates
  deterministic compliance rules

### §06.2 — Coaching Components

| Component | Path | Role |
|-----------|------|------|
| persona-logic | `aether/coaching/persona-logic/` | Persona selection and tone |
| hint-generator | `aether/coaching/hint-generator/` | Micro-hints (HUD) + structured cards |
| arbitration | `aether/coaching/arbitration/` | Priority: safety → quality → metadata → learning |

### §06.3 — Coaching Priority

When multiple hints are generated simultaneously:

```
1. Safety hints      ← always surfaced first
2. Quality hints     ← surfaced after safety
3. Metadata hints    ← lower priority
4. Learning hints    ← background / non-urgent
```

---

## §07 — adapter-runtime

`creator-os/shared/adapter-runtime/` is the only permitted entry point
for LLM and Lyria calls in the entire system.

See authoritative spec: `creator-os/constitution/amendments/llm-adapter-amendment-v1.1.md`

**Key rules:**
- `llm-client.rs` is the sole permitted LLM API call point
- `retry.rs` is the sole permitted retry implementation
- No module may call an LLM API directly
- No new file may be added without a constitution amendment
- One singleton per system — no parallel implementations

---

## §08 — ML Origin Rule (Inherited)

From Creator OS Constitution v2.5 §04 — applies in full to Aether:

> **If it depends on ML weights → Aether.**
> **If it is pure DSP or pure algorithmic → LineOS/M1.**

Aether is the destination for all ML weight-dependent computation.
Pure algorithmic processing placed in Aether is a constitutional violation.

---

## §09 — Technology Constraints

### §09.1 — Approved Libraries

| Domain | Library | Notes |
|--------|---------|-------|
| Inference (primary) | `ort` (ONNX Runtime) | Widest model compatibility |
| Inference (secondary) | `candle` | Rust-native |
| Inference (tertiary) | `tract` | Limited ops — confirm compatibility |
| Audio ML | `deepfilter-rs` | E4 neural denoising |
| Stem separation | Demucs / MDX-Net (ONNX) | Via `ort` |
| VAD | Silero VAD (ONNX) | Via `ort` |
| Image ML | Real-ESRGAN (ONNX) | Via `ort` |
| TTS | Piper | Neural TTS for E1 |
| STT | Candle Whisper | Pure Rust — NOT whisper-rs |
| Generative | LLM APIs + Lyria-3 | Via adapter-runtime only |

### §09.2 — Forbidden in Aether

```
❌ whisper-rs / whisper.cpp — C++ FFI
❌ ring — C dependencies
❌ PyTorch runtime — use ONNX
❌ Any LLM SDK outside adapter-runtime
❌ std::f32 / std::f64 in deterministic paths — use libm
❌ rand::thread_rng() in production pipelines
❌ serde_json::Value as adapter I/O
❌ Direct LineOS internal imports
❌ Direct M0 calls
❌ Persistent state between device calls
❌ UI geometry generation
❌ Writing to OLED islands or Cockpit signals directly
❌ Pure DSP or algorithmic code (belongs in LineOS — ML Origin Rule)
```

---

## §10 — Repository Structure (Canonical)

```
aether/
├── constitution/
│   └── aether-constitution.md          ← this document
├── devices/                            ← isolated ML modules
│   ├── denoise/                        # E4 — deepfilter-rs
│   │   ├── Cargo.toml
│   │   ├── README.md                   ← inference runtime choice documented
│   │   └── src/
│   │       ├── lib.rs
│   │       └── pipeline.rs
│   ├── stems/                          # Demucs / MDX-Net
│   ├── upscale/                        # Real-ESRGAN
│   ├── voice/                          # E1 — Piper
│   ├── link2suno/                      # E7 — Lyria-3
│   │   └── src/
│   │       ├── style_extractor.rs
│   │       ├── lyrics_prompt.rs
│   │       └── suno_prompt.rs
│   └── whisper/                        # Candle Whisper
├── adapters/                           ← Adapter Boundary gatekeepers
│   ├── coach-adapter/
│   ├── metadata-adapter/
│   ├── feature-adapter/
│   └── av-adapter/
├── coaching/                           ← contextual intelligence
│   ├── persona-logic/
│   ├── hint-generator/
│   └── arbitration/
└── shared/
    └── feature-vectors/                ← normalized ML output contracts
```

**This structure is immutable.** Any deviation requires a constitution amendment.

---

## §11 — CI Gates

| Gate | Check |
|------|-------|
| Aether boundary | `aether-boundary-check.sh` — no raw ML output crosses boundary |
| ML origin | `ml-origin-check.sh` — no pure DSP in aether/ |
| LLM contract | `llm-contract-check.sh` — all LLM calls through adapter-runtime |
| No `serde_json::Value` | grep check on adapter I/O signatures |
| No `whisper-rs` | grep check in aether/ |
| No direct LineOS imports | `layer-isolation-check.sh` |
| License audit | `cargo deny check licenses` — MIT/Apache2 only |
| Determinism (adapters) | adapter output schema validation test |

---

## §12 — Forbidden Work (Aether-Level)

```
❌ Aether importing LineOS internal modules
❌ Aether calling M0 directly
❌ Raw ML output crossing the Adapter Boundary without validate_output()
❌ Devices importing each other
❌ Adapters implementing retry logic (use adapter-runtime/retry.rs)
❌ Coaching blocking the DSP pipeline
❌ LLM calls outside adapter-runtime
❌ Models downloaded at runtime — all models provisioned at install time
❌ Persistent state between device or adapter calls
❌ Pure algorithmic/DSP code in Aether (ML Origin Rule)
❌ whisper-rs or any C++ FFI
❌ Stochastic output entering LineOS without passing the Adapter Boundary
```

---

## §13 — Amendment Process

1. Draft the amendment
2. Increment version (`1.0 → 1.1`)
3. Changelog entry with reason and impact
4. If Creator OS-level rules are affected → Creator OS Constitution amendment
5. CI must enforce new rules

Amendments are additive. Lead Architect approval required.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-04-09 | Initial constitution — devices, adapters, coaching, Adapter Boundary, ML inference priority, Candle Whisper decision, Link2Suno detail, coaching priority model |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `aether/constitution/aether-constitution.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED

---

*Aether imagines. LineOS executes.*
*The Adapter Boundary is the firewall between them.*
*Raw ML output never crosses it.*
