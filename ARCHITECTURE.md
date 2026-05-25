# ARCHITECTURE.md

**System:** Creator OS + LineOS + Aether + Marketplace  
**Version:** 3.5  
**Status:** 🔒 CANONICAL  
**Last updated:** 2026-05-25  
**Constitution:** `creator-os/constitution/creator-os-constitution-v2.5.md`

---

## What this document is

This is the entry point for understanding how this system is structured.  
It describes the layers, their responsibilities, their boundaries, the engine registry, the library stack, and the marketplace model.

If you are reading this for the first time, read it top to bottom.  
If you are looking for a specific component, jump to the relevant section.

This document does not replace the constitutions. It maps the territory.  
For binding rules, go to:

## v3.5 — 2026-05-25

### New Crates
- `lineos/m1/sp314-nodes` v0.1.0 — primitive DSP nodes + DspNode trait
- `pipelines/pipelineforge` v0.1.0 — Engineer Conditions → DAG routing
- `apps/runtime/openclaw` v0.1.0 — WASM executor + AudioWorklet bridge
- `lineos/m1/lineos-types` v0.1.0 — shared types, v2.9 → v3 migration

### E11 Engine
- Assembled at `lineos/m0/assets/wasm/e11.wasm` (480KB)
- Capabilities: offline mastering, stem processing, TimeAwareBehaviour,
  parameter glide, section crossfade
- SHA256 locked in `e11-manifest.json`

### Migration Status
- All 6 v2.9-dependent crates migrated to lineos-types (3a complete)
- m0-daemon wired to pipelineforge + openclaw (3b complete)
- Remaining 5 crates: 3b pending (MasteringPipeline replacement)

### Pending
- E12/E13 engines (AV pipeline — future scope)
- lineos-types 3b for telemetry, metadata, insights, rule-engine, stillair
- research/math-tuning branch created (idle)
- `creator-os/constitution/creator-os-constitution-v2.5.md`
- `lineos/constitution/lineos-constitution.md`
- `aether/constitution/aether-constitution.md`
- `marketplace/constitution/marketplace-constitution.md`

---

## The Five Layers

Creator OS is a five-layer system. Each layer has a single, non-negotiable identity.

```
┌─────────────────────────────────────────────────────────┐
│  Marketplace  ← creator layer                           │
│               extension ecosystem, asset distribution    │
└───────────────────────────┬─────────────────────────────┘
                            │ extends
┌───────────────────────────▼─────────────────────────────┐
│  Apps         ← user-facing products                    │
│               Still Air (A1), MotionCraft (A2), ...     │
└───────────────────────────┬─────────────────────────────┘
                            │ surfaces
┌───────────────────────────▼─────────────────────────────┐
│  Aether       ← ML enrichment layer                     │
│               imagination, never execution              │
│               all ML output → deterministic vectors      │
└───────────────────────────┬─────────────────────────────┘
                            │ enriches
┌───────────────────────────▼─────────────────────────────┐
│  LineOS       ← deterministic substrate                 │
│               enforces the laws                         │
│               M0 trust boundary · DSP · AV · export     │
└───────────────────────────┬─────────────────────────────┘
                            │ governed by
┌───────────────────────────▼─────────────────────────────┐
│  Creator OS   ← constitutional layer                    │
│               defines the laws                          │
└─────────────────────────────────────────────────────────┘
```

### Dependency direction (canonical, non-negotiable)

```
Creator OS   ← no dependencies
     ↑
  LineOS     ← depends on creator-os/contracts/ only
     ↑
  Aether     ← depends on LineOS interfaces + creator-os/contracts/
     ↑
  Apps       ← depend on LineOS + Aether + creator-os/shared/
     ↑
Marketplace  ← depends on creator-os/contracts/ + M0 validation API only
```

### Layer identity table

| Layer | Is | Is not |
|-------|----|--------|
| **Creator OS** | Governance, contracts, invariants | Runtime code, compute, UI |
| **LineOS** | Deterministic execution, M0, DSP, AV, Golden Blob | ML, LLM, probabilistic computation |
| **Aether** | ML enrichment, adapters, coaching, devices | DSP, UI rendering, direct LineOS calls |
| **Apps** | User-facing products (Still Air, MotionCraft) | A layer — products |
| **Marketplace** | Extension distribution, asset registry, developer ecosystem | A runtime — distributes, never executes |

---

## The Adapter Boundary

Between Aether and LineOS lies the Adapter Boundary — the firewall between probabilistic and deterministic computation.

```
Aether (stochastic: ML models, LLM calls, style extraction)
        │
        │  validate_output() against JSON schema
        ▼
[ADAPTER BOUNDARY]
        │
        │  typed, schema-compliant feature vectors ONLY
        ▼
LineOS (deterministic: DSP, AV, rule-engine, Golden Blob)
```

### The ML Origin Rule (binding)

If a component uses trained model weights — regardless of whether it runs deterministically — it belongs to Aether.  
If a component is pure mathematical DSP — no weights, no inference — it belongs to LineOS M1.

```
Aether (ML origin):              LineOS M1 (DSP origin):
  deepfilter-rs  (neural)          rustfft      (FFT math)
  Demucs         (transformer)     rubato       (resampling math)
  Silero VAD     (neural)          dasp         (DSP primitives)
  Real-ESRGAN    (neural)          symphonia    (format parsing)
  Piper          (neural TTS)      rav1e        (codec math)
  candle / ort   (inference)       mp4parse-rs  (container parsing)
```

Raw ML output never crosses the Adapter Boundary.  
Boundary contract: `creator-os/contracts/feature-vector.schema.json`

---

## Engine Registry

### Engine Numbering System

Engines are assigned permanent IDs. IDs never change.

| Range | Category | Owner | Layer |
|-------|----------|-------|-------|
| E1–E10 | Core Aether devices (ML) | Creator OS team | Aether |
| E11–E19 | Core LineOS engines (DSP) | Creator OS team | LineOS M1 |
| E20–E99 | Extended core engines | Creator OS team | Phase 2+ |
| E100–E999 | Certified marketplace engines | Third-party | Marketplace |
| E1000+ | Community engines | Community | Marketplace (unverified) |

### Core Engine Registry

| ID | Name | Layer | Library | Phase |
|----|------|-------|---------|-------|
| E1 | speakforge (voice) | Aether/devices/voice | Piper + Silero VAD | Tokyo |
| E2 | (reserved) | Aether | — | Future — transcription, sound classification |
| E3 | (reserved) | Aether | — | Future — sound classification |
| E4 | deepfilter (denoise) | Aether/devices/denoise | deepfilter-rs | Tokyo |
| E5 | stems | Aether/devices/stems | Demucs ONNX via ort | Osaka |
| E6 | upscale | Aether/devices/upscale | Real-ESRGAN ONNX via ort | Osaka |
| E7 | link2suno | Aether/devices/link2suno | candle + Lyria adapter | Tokyo |
| E8 | (reserved) | Aether | — | Future core Aether device |
| E9 | (reserved) | Aether | — | Future core Aether device |
| E10 | (reserved) | Aether | — | Future core Aether device |
| E11 | sp314-dsp | LineOS M1 | rustfft, rubato, dasp, libm | Tokyo |
| E12 | video-engine | LineOS M1 (WASM via M0) | rav1e, image-rs | Osaka |
| E13 | av-forge | LineOS M1 (WASM via M0) | mp4parse-rs, rav1e | Osaka |

**Ownership clarification — E12 and E13:**  
E12 (video-engine) and E13 (av-forge) are LineOS-owned deterministic engines. They execute inside the M0 WASM sandbox as WASM components, but they belong to the deterministic substrate — orchestrated by `av-core` (M1). They are not marketplace engines and are not subject to third-party validation.

**Build dependency note — av-core:**  
`av-core` depends only on the WIT interface definitions of E12/E13 (`engines/engine-registry/`), not on the compiled WASM artifacts. The WASM artifacts are loaded exclusively by M0 at runtime. This is why `av-core` builds before the engines in the build order.

**Rules:**
- Core engine IDs are permanent and immutable
- E1–E10: Aether devices (ML origin rule applies)
- E11–E19: LineOS deterministic engines (no ML weights)
- Any new core engine requires a constitution amendment

---

## Library Stack (Canonical 2026)

### LineOS M1 — Deterministic Rust (no ML weights permitted)

| Library | License | Role | Location |
|---------|---------|------|----------|
| `symphonia` | MIT | Audio decoding (MP3, AAC, FLAC, WAV, AIFF) | `m1/sp314-dsp` |
| `rubato` | MIT | Sample rate conversion | `m1/sp314-dsp` |
| `dasp` | MIT/Apache-2.0 | DSP primitives (interpolation, envelopes) | `m1/sp314-dsp` |
| `rustfft` | MIT/Apache-2.0 | FFT (worker thread only) | `m1/sp314-dsp` |
| `libm` | MIT | Deterministic math (no std floats in pipeline) | `m1/sp314-dsp` |
| `rav1e` | BSD-2-Clause | AV1 video encoding | `engines/video-engine` |
| `mp4parse-rs` | MPL-2.0 | MP4/MOV container parsing | `m1/av-core` |
| `image-rs` | MIT/Apache-2.0 | Image frame processing | `engines/video-engine` |

### Aether — ML Inference Rust (model weights permitted here only)

| Library | License | Role | Location |
|---------|---------|------|----------|
| `deepfilter-rs` | MIT/Apache-2.0 | Real-time neural audio denoising | `aether/devices/denoise` |
| `candle` | Apache-2.0 | HuggingFace Rust ML framework | `aether/shared` |
| `ort` | MIT | ONNX Runtime Rust bindings | `aether/shared` |
| Demucs (ONNX) | MIT | Stem separation (htdemucs) | `aether/devices/stems` |
| Silero VAD (ONNX) | MIT | Voice activity detection | `aether/devices/voice` |
| Real-ESRGAN (ONNX) | BSD-3-Clause | Image/video super-resolution | `aether/devices/upscale` |
| `piper` | MIT | Local neural TTS | `aether/devices/voice` |

### Forbidden (Constitutional Prohibition)

| Library/Tool | Reason |
|-------------|--------|
| FFmpeg | LGPL + subprocess spawn = M0 constitution violation |
| Python runtime | Wrong language layer — use candle/ort |
| PyTorch runtime | Export to ONNX, use ort instead |
| Any ML library in LineOS M1 | ML origin rule — belongs to Aether |
| `reqwest` outside adapter-runtime | Direct network calls forbidden |
| `rand::thread_rng()` in M1 | Non-deterministic |
| `std::f32::tanh()` in DSP pipeline | Use libm instead |
| `Date::now()` in deterministic code | Non-deterministic |

---

## Link2Suno — Aether Device Spec

Link2Suno bridges Creator OS with generative music platforms (Suno, Lyria).

```
Still Air (user sets style / mood / reference audio)
        │
        ▼
aether/devices/link2suno/
  ├── style_extractor.rs   ← analyzes reference audio → style feature vector
  ├── lyrics_builder.rs    ← generates structured lyric prompt
  ├── prompt_builder.rs    ← constructs Suno/Lyria prompt from vectors
  └── adapter.rs           ← calls Lyria via adapter-runtime
        │
        │  validate_output() → typed GenerativeAudioRef
        ▼
[ADAPTER BOUNDARY]
        │
        │  GenerativeAudioRef { uri, metadata, duration, style_vector }
        ▼
LineOS M1 — treats generated audio as any other audio input
  → sp314-dsp (E11) → mastering pipeline → Golden Blob
```

LineOS has no knowledge that audio was AI-generated. It processes it identically.

---

## Marketplace — Creator Layer

The Marketplace is the extension ecosystem. It distributes assets — it does not execute them. All execution is delegated to M0 sandbox.

### What Developers Can Publish

| Asset Type | ID Range | Requirement |
|-----------|----------|-------------|
| WASM Engines (DSP, video, audio) | E100–E999 | WIT interface + determinism tests |
| Aether Devices (ML modules) | E200–E299 | Feature vector output + schema contract |
| Preset Packs (EQ, mastering, color) | — | Golden Blob compatible |
| Golden Blob Packs (templates, profiles) | — | `golden-blob.schema.json` compliant |
| Noise Profiles | — | Deterministic + versioned |

### Publication Flow

```
creator publish --asset <path> --schema validate
  ├── schema validation
  ├── determinism check
  ├── reproducible build
  ├── WIT interface validation (engines)
  └── sandbox policy check
          │
          ▼
Developer signs with developer key
          │
          ▼
Uploaded to Marketplace Registry
  (digest pinning + version pinning + signature)
          │
          ▼
M0 validates before loading:
  manifest → signature → permissions → digest
```

### Marketplace Protection Rules

```
❌ Engines may not make network calls
❌ Engines may not access filesystem directly
❌ Engines may not perform ML inference (belongs to Aether devices)
❌ Engines may not import other engines
❌ Engines may not touch Cockpit or UI geometry
❌ Aether devices may not run in LineOS M1
❌ Assets may not load without M0 validation
❌ No silent auto-updates outside M0 registry
```

### Revenue Model

| Parameter | Value |
|-----------|-------|
| Developer cut | 70% |
| Creator OS cut | 30% |
| Price range | €1–€99 |
| Payout | Monthly |
| Versioning | v1, v2, v3... (old versions remain available) |

---

## Repository Layout

```
repo-root/
│
├── Cargo.toml                          # Rust workspace
├── mix.exs                             # Elixir umbrella (backend only)
├── README.md
├── ARCHITECTURE.md                     # This document
│
├── creator-os/                         # ⚖️ CONSTITUTIONAL LAYER
│   ├── constitution/
│   │   └── creator-os-constitution-v2.5.md
│   ├── invariants/
│   │   └── creator-os-invariants.md
│   ├── contracts/
│   │   ├── primitives/
│   │   │   ├── timing.schema.json
│   │   │   ├── identity.schema.json
│   │   │   └── media.schema.json
│   │   ├── golden-blob.schema.json     # type: "audio" | "av"
│   │   ├── feature-vector.schema.json  # Adapter Boundary contract
│   │   ├── voice.schema.json
│   │   ├── narrative.schema.json
│   │   ├── coach-narrative.schema.json
│   │   ├── pack.schema.json
│   │   ├── av.schema.json
│   │   ├── av-timeline.schema.json
│   │   ├── av-export.schema.json
│   │   └── av-measurement.schema.json
│   └── shared/
│       └── adapter-runtime/            # ONLY LLM / Lyria entry point
│           ├── Cargo.toml
│           └── src/
│               ├── llm-client.rs
│               ├── adapter-traits.rs
│               ├── validation.rs
│               ├── retry.rs
│               └── providers/
│                   ├── mistral.rs      # 🔒 locked — local, offline
│                   ├── claude.rs       # 🔒 locked — cloud, opt-in
│                   └── lyria.rs        # 🔒 locked — cloud, opt-in
│                   # Lyria provider requires explicit user opt-in
│                   # and is subject to Google third-party terms.
│
├── backend/                            # 🧠 STATELESS GATEWAY (Elixir/Phoenix)
│   ├── mix.exs
│   └── lib/
│       ├── gateway/                    # WebSocket / HTTP facade
│       ├── auth/                       # Tokens, session boundaries
│       ├── rate_limit/
│       ├── m0_bridge/                  # IPC bridge → m0 daemon (JSON only)
│       ├── integrations/               # Stripe, OAuth
│       └── telemetry/                  # Log streaming (pure forwarder)
│
├── lineos/                             # 🚀 DETERMINISTIC OS CORE (Rust)
│   ├── constitution/
│   ├── architecture/
│   ├── plan/phase-1/task-decomposition.md
│   ├── m0/                             # 🟡 TRUST BOUNDARY
│   │   ├── api/
│   │   │   └── m0-api.schema.json      # ❄️ Frozen
│   │   ├── m0-daemon/
│   │   ├── sandbox/                    # WASM execution (Phase 2+)
│   │   └── assets/
│   │       ├── wasm/
│   │       │   ├── sp314-dsp.wasm      # E11
│   │       │   ├── video-engine.wasm   # E12
│   │       │   └── av-forge.wasm       # E13
│   │       ├── engines/                # Native binaries
│   │       └── schemas/
│   └── m1/                             # 🔵 DETERMINISTIC PROCESSING
│       ├── sp314-dsp/                  # E11 — 8-stage DSP pipeline
│       │   └── src/
│       │       ├── pipeline.rs         # symphonia + rubato + dasp + libm
│       │       ├── filters.rs
│       │       └── math.rs             # libm only, no std floats
│       ├── av-core/                    # AV timeline + sync + composition
│       │   └── src/
│       │       ├── sync.rs             # Master clock, drift correction
│       │       ├── composition.rs
│       │       └── export.rs           # Golden Blob AV
│       ├── telemetry/
│       │   ├── ebu-r128.rs
│       │   └── video-metrics.rs
│       ├── metadata/
│       ├── insights/
│       └── rule-engine/                # Deterministic compliance — NO ML
│
│   └── shared/schema/                  # LineOS-internal only
│       ├── ebu-r128.schema.json
│       ├── bmr-128.schema.json
│       ├── project.schema.json
│       └── audit.schema.json
│
├── aether/                             # 🌌 ML ENRICHMENT LAYER
│   ├── constitution/
│   ├── devices/
│   │   ├── denoise/                    # E4 — deepfilter-rs
│   │   ├── stems/                      # E5 — Demucs ONNX
│   │   ├── upscale/                    # E6 — Real-ESRGAN ONNX
│   │   ├── voice/                      # E1 — Piper + Silero VAD
│   │   └── link2suno/                  # E7 — style extraction + Lyria
│   │       └── src/
│   │           ├── style_extractor.rs
│   │           ├── lyrics_builder.rs
│   │           ├── prompt_builder.rs
│   │           └── adapter.rs
│   ├── adapters/
│   │   ├── coach-adapter/
│   │   ├── metadata-adapter/
│   │   ├── feature-adapter/
│   │   └── av-adapter/
│   ├── coaching/
│   │   ├── persona-logic/
│   │   ├── hint-generator/
│   │   └── arbitration/
│   └── shared/
│       ├── candle/                     # HuggingFace Rust ML
│       ├── ort/                        # ONNX Runtime bindings
│       └── feature-vectors/
│
├── crates/                             # 📦 SHARED HELPERS
│   ├── creator-pool/
│   ├── gallery-identity/
│   └── lineos-utils/
│
├── engines/                            # ⚙️ WASM COMPONENT ENGINES
│   ├── engine-registry/
│   │   ├── voice.wit                   # E1
│   │   ├── video-engine.wit            # E12
│   │   └── av-timeline.wit             # E13
│   ├── speakforge/                     # E1
│   ├── video-engine/                   # E12 — rav1e + image-rs
│   └── av-forge/                       # E13 — mp4parse-rs + rav1e
│
├── pipelines/                          # ⛓️ DAG EXECUTION
│   ├── pipelineforge/
│   │   ├── dags/
│   │   │   ├── stillair-mastering.dag.json
│   │   │   └── stillair-av-mastering.dag.json
│   │   └── schemas/
│   │       ├── dag.schema.json
│   │       └── dag-av.schema.json
│   └── assetgraph/
│
├── apps/                               # 🕹️ USER-FACING APPS
│   ├── stillair/                       # A1 — audio mastering + AV preview
│   │   ├── cockpit/src/panels/
│   │   │   ├── mastering/
│   │   │   ├── av-preview/             # Read-only — no editing
│   │   │   ├── link2suno/              # Generative music panel
│   │   │   └── radio_mix/
│   │   └── adapter-registry.json
│   └── motioncraft/                    # A2 — AV timeline editor (future)
│       # Constitutional boundary: Still Air previews, MotionCraft edits
│
├── marketplace/                        # 🏪 CREATOR LAYER
│   ├── registry/
│   │   ├── engine-registry.json        # All published engines + metadata
│   │   ├── pack-registry.json
│   │   └── device-registry.json
│   ├── validator/                      # creator publish CLI
│   │   └── src/
│   │       ├── schema-check.rs
│   │       ├── determinism-check.rs
│   │       ├── wit-check.rs
│   │       └── sandbox-check.rs
│   ├── sdk/                            # Developer SDK (Phase 2)
│   │   ├── engine-template/
│   │   ├── device-template/
│   │   └── pack-template/
│   └── constitution/
│       └── marketplace-constitution.md
│
├── gallery/                            # 🖼️ CREATOR POOL (storage)
│   ├── public-gallery/
│   └── creator-pool/
│       └── src/
│           ├── bundles/
│           ├── vectors/
│           ├── registry/
│           └── locks/
│
└── infra/
    ├── edge/caddy/
    └── ci/checks/
        ├── determinism-check.sh
        ├── schema-check.sh
        ├── layer-isolation-check.sh
        ├── llm-contract-check.sh
        ├── wasm-component-check.sh
        ├── aether-boundary-check.sh
        ├── av-sync-check.sh
        ├── av-boundary-check.sh
        └── marketplace-check.sh        # WIT + signature validation
```

---

## Layer Responsibilities

### `creator-os/` — Constitutional layer

Single source of truth for all contracts. `adapter-runtime` is the only crate permitted to call LLMs or Lyria. All contracts in `creator-os/contracts/` — `lineos/shared/schema/` contains LineOS-internal schemas only.

---

### `backend/` — Stateless gateway (Elixir/Phoenix)

Routes, authenticates, proxies. Does not process audio, video, or ML output. Chosen for BEAM's concurrency model — WebSocket fan-out, pub/sub, real-time telemetry streaming at scale. Communicates with LineOS via `m0-api.schema.json` only — no Rust type sharing.

---

### `lineos/` — Deterministic OS core

**M0:** Trust boundary. Validates all marketplace assets before loading (manifest → signature → permissions → digest). All traffic passes through it.

**M1:** Pure deterministic processing.
- `sp314-dsp` (E11) — 8-stage audio mastering → Golden Blob audio
- `av-core` — AV timeline, sync, composition → Golden Blob AV. Orchestrates E12/E13 via M0 IPC — never loads WASM directly.
- `telemetry/` — LineOS M1 implements ITU-R BS.1770-4 as the canonical loudness algorithm. EBU R128 and all platform loudness targets (Spotify, YouTube, Apple Podcasts, broadcast) are deterministic transforms of BS.1770-4. The math lives in M1. The policy lives in the Golden Blob.
- `rule-engine` — deterministic compliance. No LLM. No ML weights. No Aether calls.

**IPC performance note:** `av-core` dispatches to E12/E13 via M0 IPC. Batch dispatch (multiple frames per call) is the recommended pattern for video. Batching is implemented by `av-core` — engines must accept batched input. Shared memory (mmap) is a Phase 2 option. Current design prioritises correctness over throughput.

---

### `aether/` — ML enrichment layer

The imagination layer. Enriches content with ML. Never executes DSP.

**ML Origin Rule enforced here:** deepfilter-rs, Demucs, Real-ESRGAN, Silero VAD, Piper — all ML-origin, all live here regardless of runtime behavior.

**Link2Suno (E7):** Extracts style from reference audio, builds Lyria/Suno prompts, returns `GenerativeAudioRef` through the Adapter Boundary. LineOS treats generated audio identically to any other audio input.

---

### `marketplace/` — Creator layer

Distributes, validates, versions. Never executes. M0 sandbox enforces all isolation at load time.

---

### Engine Lifecycle

All engines — core and marketplace — follow a strict lifecycle:

```
publish → validate → pin → load → (optional) deprecate → remove
```

- **publish:** Developer submits via `creator publish` CLI
- **validate:** Schema, determinism, WIT, sandbox checks — all must pass
- **pin:** Engine pinned in M0 registry with digest + version
- **load:** M0 validates manifest, signature, permissions, digest — then sandboxes
- **deprecate:** Marked deprecated — still loadable by existing users
- **remove:** Explicit removal — requires developer or Creator OS action

Deprecated engines remain pinned and loadable until explicitly removed. No engine is silently removed. No engine auto-updates without a new pinned registry entry.

---

### `crates/` — Layer-agnostic shared helpers

Shared Rust crates with at least two distinct consumers.

**Isolation rule:** `crates/` may not depend on `aether/` or `lineos/m1/`. They must remain layer-agnostic — usable by any layer without creating cross-layer coupling. If a crate only has one consumer, it stays inside that consumer's directory.

---

### `pipelines/` — DAG execution

Declarative orchestration. No business logic. `pipelineforge/` runs JSON-defined DAGs via a pure state machine — it runs DAGs, it does not interpret them. Aether adapters may contribute configuration to a DAG but must not introduce nondeterministic branching. DAGs are defined as JSON (`dags/*.dag.json`) and validated against `dag.schema.json`.

---

### `gallery/` — Creator pool (storage)

Physical storage layer only: bundles, vectors, registry, locks. No product logic, no business rules. `creator-pool/` stores content-addressable assets. `public-gallery/` handles public-facing discovery.

---

### `apps/` — User-facing products

**Still Air (A1):** Mastering + AV preview + Link2Suno. No timeline editing.  
**MotionCraft (A2):** Full AV editing — future, constitutional boundary with A1.

---

## Data Flows

### Audio Mastering

```
1.  Drop audio → Cockpit
2.  Cockpit → M0 → sp314-dsp (E11) → 8-stage pipeline
3.  Golden Blob (mastered FLAC + QualityMetrics)
3a. M0 audit log: start
4.  Golden Blob → telemetry → EBU R128
5.  EBU R128 → metadata → BMR-128 report
6.  QualityMetrics → insights → compliance
7.  Compliance → rule-engine → hints
8.  Hints → Coach Island

    ── Aether path (parallel) ──────────────────────────────
9.  QualityMetrics → feature-adapter → feature-vector
10. feature-vector → coach-adapter → adapter-runtime → LLM
11. LLM → validate_output() → CoachNarrative
12. CoachNarrative → PersonaEventBus → Coach Island
    ────────────────────────────────────────────────────────

13. Export → FLAC + reports (via M0 policy gate)
13a. M0 audit log: outcome
```

### AV Mastering

```
1.  Drop AV file → Cockpit
2.  Cockpit → M0 → av-core (timeline parsing)
3.  av-core → M0 dispatch (parallel):
    ├── Audio → sp314-dsp (E11)
    └── Video → video-engine (E12) + av-forge (E13)
4.  Results → av-core → Golden Blob (AV container)
5a. M0 audit log: start
6.  Golden Blob → telemetry (EBU R128 + video metrics)
7.  Metrics → rule-engine → compliance → Coach Island
    ── Aether path (parallel) ──────────────────────────────
8.  Metrics → av-adapter → AV feature-vector → LLM → Coach Island
    ────────────────────────────────────────────────────────
9.  Export → AV container + reports (via M0 policy gate)
```

### Link2Suno

```
1.  User sets style/mood + optional reference audio
2.  link2suno/style_extractor → style feature vector
3.  link2suno/lyrics_builder → lyric prompt
4.  link2suno/prompt_builder → Suno/Lyria prompt
5.  adapter.rs → adapter-runtime → Lyria/Suno API
6.  API response → validate_output() → GenerativeAudioRef
        [ADAPTER BOUNDARY]
7.  GenerativeAudioRef → M0 → sp314-dsp (E11) → mastering
8.  Golden Blob → export
    LineOS never knows audio was AI-generated.
```

---

## Dependency Direction (Binding)

```
creator-os/    →  nothing
lineos/        →  creator-os/contracts/ only
aether/        →  lineos/ interfaces + creator-os/contracts/
apps/          →  lineos/ + aether/ + crates/ + creator-os/shared/
marketplace/   →  creator-os/contracts/ + M0 validation API + crates/ (validator only)
engines/       →  crates/
pipelines/     →  engines/ + crates/
backend/       →  lineos/m0/api (JSON schema only)
```

**Absolutely forbidden:**
- ML libraries in `lineos/` — ML origin rule
- DSP code in `aether/` — belongs to M1
- `lineos/` → `aether/` — no ML knowledge in deterministic substrate
- `aether/` → `lineos/` internals — feature vectors only
- Marketplace engines → LineOS internals — M0 sandbox enforces
- LLM calls outside `adapter-runtime`
- Rust type sharing across Elixir/Rust boundary

---

## Contract System

All contracts in `creator-os/contracts/`. `lineos/shared/schema/` = LineOS-internal only.

| Contract | Consumers |
|----------|-----------|
| `golden-blob.schema.json` | LineOS, Aether, Apps — type `"audio"` or `"av"`. Loudness metrics store both BS.1770-4 canonical values and EBU R128 derived values. The telemetry module computes BS.1770-4 measurements once — all EBU R128 and platform-specific values are derived from these. No re-measurement occurs downstream. |
| `feature-vector.schema.json` | Aether → LineOS — Adapter Boundary |
| `av-measurement.schema.json` | LineOS, Aether, Apps — public contract |
| `av.schema.json` | LineOS, Apps |
| `av-timeline.schema.json` | LineOS, Apps |
| `av-export.schema.json` | LineOS, Apps |
| `coach-narrative.schema.json` | Aether → Apps |
| `pack.schema.json` | Marketplace, Apps |

---

## CI Gates (All Hard Failures)

| Gate | What it enforces |
|------|-----------------|
| `schema-check.sh` | All contracts valid |
| `layer-isolation-check.sh` | No cross-layer internal imports; no ML in M1; no DSP in Aether |
| `llm-contract-check.sh` | All LLM/Lyria calls via adapter-runtime only |
| `wasm-component-check.sh` | No engine internals in UI |
| `aether-boundary-check.sh` | No raw ML crosses Adapter Boundary |
| `determinism-check.sh` | Same input + seed → identical output |
| `av-sync-check.sh` | Timebase + drift invariants |
| `av-boundary-check.sh` | Video never bypasses M0 |
| `marketplace-check.sh` | WIT interface + signature on marketplace engines |

To run locally: `scripts/validate-all.sh`

---

## Build Order

```
1.  crates/
2.  creator-os/shared/adapter-runtime/
3.  aether/shared/ (candle, ort)
4.  lineos/m0/
5.  lineos/m1/ audio: sp314-dsp → telemetry → metadata → insights → rule-engine
6.  lineos/m1/ AV: av-core
7.  engines/ E1, E12, E13 → lineos/m0/assets/wasm/
8.  aether/devices/ E4, E5, E6, E7
9.  aether/adapters/ + aether/coaching/
10. pipelines/
11. marketplace/validator/
12. apps/stillair/
13. backend/
```

---

## Future (Osaka+)

| Item | Phase | Notes |
|------|-------|-------|
| MotionCraft A2 | Osaka | Full AV editing — NOT in Still Air |
| Marketplace SDK v1 | Osaka | Engine + device + pack templates |
| E100+ marketplace engines | Osaka | Third-party certified engines |
| Shared memory IPC | Osaka | For high-frequency AV frame dispatch |
| GPU-optional pipelines | Osaka | Heavy ML (Hetzner on-demand) |
| Windows support | Osaka | Tauri cross-platform |
| Web WASM | Osaka | Browser subset |
| E1000+ community tier | Fukuoka | Unverified community engines |

---

## Where to Go Next

| I want to understand... | Go to... |
|-------------------------|----------|
| The binding rules | `creator-os/constitution/creator-os-constitution-v2.5.md` |
| LineOS internals | `lineos/architecture/lineos-architecture.md` |
| The Aether layer | `aether/constitution/aether-constitution.md` |
| The Marketplace rules | `marketplace/constitution/marketplace-constitution.md` |
| The DSP pipeline | `lineos/m1/sp314-dsp/src/pipeline.rs` |
| The AV core | `lineos/m1/av-core/src/lib.rs` |
| Deterministic rule engine | `lineos/m1/rule-engine/` |
| LLM coaching & persona | `aether/coaching/` |
| The Golden Blob spec | `creator-os/contracts/golden-blob.schema.json` |
| The Adapter Boundary | `creator-os/contracts/feature-vector.schema.json` |
| LLM adapter rules | `creator-os/constitution/amendments/llm-adapter-amendment-v1.1.md` |
| Engine registry | This document — Engine Registry section |
| Library stack | This document — Library Stack section |
| The build sequence | `lineos/plan/phase-1/task-decomposition.md` |
| Still Air constitution | `apps/stillair/constitution/stillair-constitution.md` |

---

*If it is not in a constitution, it does not exist.*  
*If it is not in a contract, it is not an interface.*  
*If it bypasses M0, it is a security violation.*  
*If it crosses the Adapter Boundary unvalidated, it is a determinism violation.*  
*If it uses ML weights, it belongs to Aether — not LineOS.*
