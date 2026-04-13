# Creator OS — Architecture Constitution

**Document:** `creator-os/constitution/creator-os-constitution.md`
**Version:** 2.6
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** OS-level — supersedes all layer and module constitutions
**Inherits:** `creator-os/invariants/creator-os-invariants.md` v1.1
**Supersedes:** Creator OS Constitution v2.5

---

## Preamble

This document is the architectural constitution of Creator OS.
It defines what the system is, how it is structured, and what rules govern its construction.

Every layer, every module, every crate, every contract, every developer decision is bound by this document.

If any layer or module constitution conflicts with this document, this document wins.
If this document conflicts with `creator-os-invariants.md`, the invariants win.

There is no higher authority than the invariants.

---

## §01 — What Creator OS Is

Creator OS is a governed, deterministic, local-first operating system for audio, video, and image production.

It is not a single application. It is a four-layer system:

```
Creator OS   ← constitutional layer (this document)
     ↑
  LineOS     ← deterministic execution substrate
     ↑
  Aether     ← ML enrichment layer
     ↑
  Apps       ← user-facing products
     ⟷
Marketplace  ← ecosystem layer (peer of Apps, not a dependency)
```

Creator OS defines the laws. LineOS enforces them. Aether enriches within them.
Applications surface them. Marketplace extends them.

### §01.1 — What Creator OS Is Not

- ❌ A monolith — no module knows the internals of another
- ❌ A cloud-first platform — cloud is optional and always opt-in
- ❌ A plugin system — modules are independent products, not extensions of a host
- ❌ A framework — shared infrastructure provides primitives, not product opinions
- ❌ A probabilistic system — stochastic computation is contained entirely within Aether

### §01.2 — Layer Constitutions

Every layer must define its own constitution.
Layer constitutions must inherit from this document and `creator-os-invariants.md`.
No layer constitution may define rules that contradict this document.

Current layer constitutions:
- `lineos/constitution/lineos-constitution.md`
- `aether/constitution/aether-constitution.md`
- `apps/stillair/constitution/stillair-constitution.md`
- `marketplace/constitution/marketplace-constitution.md`

### §01.3 — Core Invariants (Inherited)

This constitution inherits the immutable invariants from
`creator-os/invariants/creator-os-invariants.md` v1.1.

The most critical invariants for Creator OS:

- **Deterministic output** — same input + seed → identical output, always
- **Local-first** — all core functionality runs offline
- **Privacy by architecture** — user data never leaves device without explicit opt-in
- **No training on user data** — absolute prohibition
- **Contract-only communication** — no direct cross-module function calls
- **No silent failures** — `let _ =` is a build failure
- **ML isolation** — ML output enters LineOS only as validated feature vectors

Any violation of these invariants is a constitutional breach.

---

## §02 — The Four-Layer Stack

### §02.1 — Layer Definitions

| Layer | Role | Nature |
|-------|------|--------|
| **Creator OS** | Governance, contracts, invariants | Constitutional — no runtime code |
| **LineOS** | Deterministic execution, M0, DSP, AV | Deterministic — Rust, WASM |
| **Aether** | ML enrichment, adapters, coaching | Probabilistic — contained |
| **Apps** | User-facing products | Consumers of all three layers |
| **Marketplace** | Ecosystem, engines, packs | Peer of Apps — not a dependency |

### §02.2 — Dependency Direction (Non-Negotiable)

```
Creator OS   ← no dependencies
     ↑
  LineOS     ← depends on creator-os/contracts/ only
     ↑
  Aether     ← depends on LineOS interfaces + creator-os/contracts/
     ↑
  Apps       ← depend on all three layers
     ⟷
Marketplace  ← peer of Apps — apps function fully without Marketplace
```

A layer may never import from a layer above it.
Enforced by `layer-isolation-check.sh` on every commit.

### §02.3 — The Adapter Boundary

Between Aether and LineOS lies the Adapter Boundary — the firewall between
probabilistic and deterministic computation.

```
Aether (stochastic)
        │  validate_output() against JSON schema
        ▼
[ADAPTER BOUNDARY]
        │  typed, schema-compliant feature vectors only
        ▼
LineOS (deterministic)
```

Raw ML or LLM output must never cross this boundary.
After this boundary, all data is treated as deterministic input.
The boundary contract is `creator-os/contracts/feature-vector.schema.json`.

---

## §03 — Execution Authority Model

Runtime orchestration authority belongs **exclusively** to `pipelines/`.
No App, Aether module, or LineOS module may define step ordering or execution
sequence outside this layer.

### §03.1 — Layer Roles at Runtime

| Layer | Runtime role | Must not |
|-------|-------------|----------|
| Apps | Express intent, trigger pipelines, render output | Define execution sequence |
| Aether | Enrich steps with ML when invoked by a pipeline | Orchestrate or call LineOS directly |
| LineOS | Execute individual steps deterministically | Define its own invocation order |
| Pipelines | Define step ordering, data routing, boundary crossings | Contain transformation logic or ML |

### §03.2 — Pipeline Definition Rules

Pipelines are **declarative DAGs only** — JSON or YAML files.
No Rust transformation code lives in the Pipeline layer.

Permitted: step ordering, routing decisions, parameter passing, boundary declarations.
Forbidden: audio/video transformation, ML inference, business logic, direct M0 calls.

CI gate: `pipeline-logic-check.sh` — non-trivial data mutation in a DAG = build failure.

Full specification: `pipelines/constitution/pipelines-constitution.md`

---

## §04 — Application Registry

| ID | Application | Domain | Status |
|----|------------|--------|--------|
| A1 | Still Air | Audio mastering + AV preview | Active — Phase 1 |
| A2 | MotionCraft | AV timeline editing | Future — Phase 2+ |
| A3 | VoiceForge | AI voice synthesis | Future |
| A4 | Wonderus | Visual storytelling | Future |
| A5 | ScriptFlow | Script & story | Future |
| A6 | Logos | Brand identity | Future |

**Rules:**
- No new application may be added without a constitution amendment
- Application IDs are permanent. They never change.
- Still Air (A1) is the only app in Phase 1. All others are Phase 2+.
- The boundary between A1 and A2 is constitutional: Still Air **previews** AV.
  MotionCraft **edits** AV. This boundary may not be violated without amendment.

### §03.1 — Engine & Device Registry

Engine IDs are permanent once assigned. The full engine registry and namespace
rules are defined in `ARCHITECTURE.md`. The following ranges are constitutionally reserved.

**Permanent ID ranges:**

| Range | Category | Owner | Layer |
|-------|----------|-------|-------|
| E1–E10 | Core Aether devices (ML origin) | Creator OS team | Aether |
| E11–E19 | Core LineOS engines (DSP, deterministic) | Creator OS team | LineOS |
| E20–E99 | Extended core | Creator OS team | Apps / Pipelines |
| E100–E999 | Marketplace engines | Third-party | Marketplace |
| E1000+ | Community engines | Community | Marketplace |

**Active core engines:**

| ID | Name | Layer | Purpose |
|----|------|-------|---------|
| E1 | SpeakForge | Aether | Voice synthesis (Piper + Silero VAD) |
| E4 | Denoise | Aether | Audio denoising (deepfilter-rs) |
| E7 | Link2Suno | Aether | Style extraction for Suno (Lyria-3) |
| E11 | sp314-dsp | LineOS M1 | 8-stage audio mastering engine |
| E12 | VideoEngine | LineOS engine | Deterministic frame operations |
| E13 | AV Forge | LineOS engine | AV composition + sync |

**Rules:**
- New core engines (E1–E19) require a constitution amendment
- Marketplace engines (E100+) must be signed and pass M0 validation before loading
- All-or-nothing loading — no partial engine loads

---

## §05 — ML Origin Rule (Constitutional)

> **If it depends on ML weights → Aether.
> If it is pure DSP or pure algorithmic → LineOS/M1.**

This rule is the definitive answer to where any library or component belongs.
It is not a guideline — it is a constitutional rule enforced by CI.

**Canonical classification:**

| Component | Layer | Reason |
|-----------|-------|--------|
| `symphonia` | LineOS / M1 | Pure audio decoding — covers WAV, MP3, FLAC, OGG, AIFF, AAC |
| `rubato`, `dasp`, `rustfft` | LineOS / M1 | Pure algorithmic — no weights |
| `imageproc`, `fastimageresize` | LineOS engines | Pure algorithmic — no weights |
| `rav1e`, `mp4parse`, `matroska-rs` | LineOS engines | Pure algorithmic — no weights |
| `crossbeam`, `parking_lot` | LineOS / M1 | Deterministic concurrency |
| `libm` | LineOS / M1 | Deterministic float math |
| `deepfilter-rs` | Aether | Runs neural network weights |
| `Demucs`, `Silero VAD`, `Real-ESRGAN` (ONNX) | Aether | ML model weights |
| Candle Whisper | Aether | Neural speech-to-text — weights |
| `candle`, `ort`, `tract` | Aether | ML inference runtimes |
| `Piper` | Aether | Neural TTS — weights |
| LLM APIs, Lyria-3 | Aether via adapter-runtime | Stochastic by definition |

Placing ML weights in LineOS, or pure DSP in Aether, is a constitutional breach
and a build failure.

---

## §06 — Contract System

All contracts live in `creator-os/contracts/`. This is the only authoritative location.
No contract may be defined outside this directory.

### §05.1 — Contract Index

| Contract | Consumers | Description |
|----------|-----------|-------------|
| `golden-blob.schema.json` | LineOS, Aether, Apps | Master output — type `"audio"` or `"av"` |
| `feature-vector.schema.json` | Aether → LineOS | Adapter Boundary contract |
| `voice.schema.json` | Aether, Apps | Voice synthesis parameters |
| `narrative.schema.json` | Aether, Apps | Narrative structure |
| `coach-narrative.schema.json` | Aether → Apps | Coaching output |
| `pack.schema.json` | Marketplace, Apps | Pack definition |
| `av.schema.json` | LineOS, Apps | AV bundle contract |
| `av-timeline.schema.json` | LineOS, Apps | Timeline, tracks, clips |
| `av-export.schema.json` | LineOS, Apps | Container, codecs, export |
| `av-measurement.schema.json` | LineOS, Aether, Apps | Unified AV metrics |
| `primitives/timing.schema.json` | All | Timing primitives |
| `primitives/identity.schema.json` | All | Identity primitives |
| `primitives/media.schema.json` | All | Media primitives |

### §05.2 — Contract Rules

- Every field is required unless explicitly marked `optional`
- Contracts evolve additively — new fields may be added, existing fields may never be removed
- Breaking changes require a new versioned contract: `golden-blob-v2.schema.json`
- Every module must validate all incoming and outgoing messages at runtime
- Validation failures are hard errors — never silently ignored
- Contracts are language-agnostic — Rust crates validate against them, they do not define them
- Silent schema drift — changing output shape without a version increment — is a build failure

### §05.3 — LineOS Internal Schemas

`lineos/shared/schema/` contains LineOS-internal schemas only:
`ebu-r128`, `bmr-128`, `project`, `audit`.

These are not public contracts. Any schema used by more than one layer
belongs in `creator-os/contracts/`, not here.

---

## §07 — Shared Infrastructure

### §06.1 — adapter-runtime

`creator-os/shared/adapter-runtime/` is the only permitted entry point for LLM and Lyria calls.

- `llm-client.rs` — sole permitted LLM API call point
- `adapter-traits.rs` — `LLMAdapter` trait definition
- `validation.rs` — output schema validation
- `retry.rs` — sole permitted retry implementation
- `providers/` — provider-specific implementations

**Rules:**
- No module may call an LLM API directly — all calls route through `llm-client.rs`
- No new file may be added to `adapter-runtime/` without a constitution amendment
- The adapter-runtime is a shared singleton — no parallel implementation exists
- Enforced by `llm-contract-check.sh`

### §06.2 — Shared Crates

Shared Rust crates with at least two distinct consumers live in `crates/`.

- `creator-pool/` — indexing and migration logic
- `gallery-identity/` — billing and ownership
- `lineos-utils/` — math, types, BMR-128 helpers

**Rule:** A crate with only one consumer stays inside that consumer's directory.
Extraction to `crates/` requires at least two distinct consumers.

---

## §08 — Technology Stack

### §07.1 — Approved Technologies

| Domain | Technology | Rationale |
|--------|-----------|-----------|
| Deterministic execution | Rust | Memory safety, performance, WASM |
| WASM engines | `wasm32-unknown-unknown` + wasm-opt | Sandboxed, portable, version-pinned |
| Frontend UI | Leptos 0.8+ | Rust/WASM, reactive signals |
| UI bundler | Trunk | WASM + asset pipeline |
| CSS | CSS-in-Rust | No plain CSS files |
| Stateless gateway | Elixir / Phoenix | BEAM concurrency, WebSocket, pub/sub |
| Database | PostgreSQL + SQLx | Typed queries, migrations |
| Reverse proxy | Caddy | JSON config, localhost-only enforcement |
| Container runtime | Podman (rootless) | Rootless, Quadlet + systemd |
| Schema validation | JSON Schema (runtime) | Language-agnostic contracts |
| Dependency audit | `cargo deny` | Zero violations required |

### §07.2 — Technology Boundary Rules

- Elixir is permitted **only** in `backend/` — the stateless gateway layer
- Elixir must never import Rust types directly — all communication via JSON contracts
- Rust is the only language permitted in `lineos/`, `aether/`, `engines/`, `crates/`, and `apps/`
- No new language may be introduced without a constitution amendment

### §07.3 — Forbidden Technologies and Dependencies (Constitutional)

Violation of any item in this section is a build failure. No exceptions without a documented constitution amendment.

**Forbidden runtimes and processes:**
- `ffmpeg` — external process, non-deterministic across versions
- Any Python runtime in the execution path
- Any PyTorch runtime — use ONNX exports via `candle` or `ort`
- Any subprocess spawned outside M0 mediation
- `whisper-rs` / `whisper.cpp` — C++ FFI; use Candle Whisper (pure Rust) instead
- `ring` crate — C dependencies in signing path; use `ed25519-dalek` + `blake3`

**Forbidden dependency patterns:**
- `reqwest` or any HTTP client outside `adapter-runtime`
- Any LLM SDK called outside `adapter-runtime`
- Any GPL-licensed crate — MIT or Apache 2.0 only
- `rand::thread_rng()` or `OsRng` in production pipelines
- `std::f32::tanh()` or `std::f64` methods in DSP pipeline code — use `libm`
- `serde_json::Value` as Engine input type — typed structs only
- `serde_yaml` in LineOS or Aether — permitted in Apps layer only
- `hound`, `lewton`, `claxon` — redundant; `symphonia` covers all formats
- `rodio` in any DSP or processing path — UI preview only

**Forbidden code patterns:**
- `let _ =` anywhere in production code — silent failure
- Per-frame IPC calls to E12/E13 — batch dispatch only (performance invariant)
- Any ORM that generates non-deterministic queries
- Any CSS framework with dynamic class generation (no Tailwind JIT in production builds)

---

## §09 — LLM Adapter Rules

LLM Adapters belong exclusively to the Aether ML Enrichment Layer.
Full specification: `creator-os/constitution/amendments/llm-adapter-amendment-v1.1.md`

Key rules:
- Adapters implement `LLMAdapter` trait — compile-time enforcement
- Input and output are strictly typed — no `serde_json::Value`
- `validate_output()` must be called before any output crosses the Adapter Boundary
- Adapters must not execute inside LineOS, Creator OS, or any engine
- Adapters must not generate UI geometry or write to OLED islands
- Adapters must not include PII without `"pii_policy": "allowed"` in registry

---

## §10 — Repository Structure (Canonical)

```
repo-root/
├── creator-os/         ← constitutional layer (this document)
├── backend/            ← stateless gateway (Elixir/Phoenix)
├── lineos/             ← deterministic OS core (Rust)
├── aether/             ← ML enrichment layer (Rust)
├── crates/             ← shared helpers (Rust, ≥2 consumers)
├── engines/            ← WASM component engines (Rust)
├── pipelines/          ← DAG execution (Rust)
├── apps/               ← user-facing applications
├── marketplace/        ← ecosystem layer
├── gallery/            ← creator layer (storage)
└── infra/              ← edge, CI, deployment
```

**Rule:** This structure is immutable. Any deviation requires a constitution amendment.
Full canonical file tree: `ARCHITECTURE.md`

---

## §11 — Build Order (Binding)

```
1.  crates/
2.  creator-os/shared/adapter-runtime/
3.  lineos/m0/
4.  lineos/m1/ audio: sp314-dsp → telemetry → metadata → insights → rule-engine
5.  lineos/m1/ AV: av-core
6.  engines/  (E11 sp314-dsp, E12, E13 → lineos/m0/assets/wasm/)
7.  aether/devices/
8.  aether/adapters/ + aether/coaching/
9.  pipelines/
10. apps/stillair/
11. backend/
```

No phase may begin before the previous is complete.

### §10.1 — Shipping Rule

No application may ship commercially unless:
1. Its constitution is locked (`Status: 🔒 LOCKED`)
2. The constitution explicitly references this document version
3. All CI checks pass on the shipping commit
4. The `adapter-registry.json` is complete and valid

---

## §12 — CI Requirements (Binding)

All CI gates are hard failures. There are no warnings.

| Gate | Script | What it checks |
|------|--------|---------------|
| Schema validation | `schema-check.sh` | All contracts valid |
| Layer isolation | `layer-isolation-check.sh` | No cross-layer internal imports |
| LLM contract | `llm-contract-check.sh` | All LLM calls through adapter-runtime |
| WASM boundary | `wasm-component-check.sh` | No engine internals imported by UI |
| Aether boundary | `aether-boundary-check.sh` | No raw ML output crosses Adapter Boundary |
| ML origin | `ml-origin-check.sh` | No ML weights in LineOS, no pure DSP in Aether |
| Pipeline logic | `pipeline-logic-check.sh` | No transformation logic in DAG files |
| Layer orchestration | `layer-orchestration-check.sh` | No step-sequencing in Apps/Aether/LineOS |
| Determinism | `determinism-check.sh` | Same input + seed → bit-identical output |
| AV sync | `av-sync-check.sh` | AV sync invariants (timebase, drift) |
| AV boundary | `av-boundary-check.sh` | Video processing never bypasses M0 |
| Marketplace policy | `marketplace-policy-check.sh` | All E100+ engines signed + pinned |
| No silent failures | inline grep | `let _ =` → build failure |
| License audit | `cargo deny check licenses` | MIT/Apache2 only |

To run all checks locally: `scripts/validate-all.sh`

---

## §13 — Forbidden Work (OS-Level)

```
❌ Adding a new application without a constitution amendment
❌ Adding a new core engine (E1–E19) without a constitution amendment
❌ Changing application IDs or domain assignments
❌ Changing engine IDs once assigned
❌ Defining contracts outside creator-os/contracts/
❌ Calling LLM APIs outside adapter-runtime
❌ ML computation or ML weights inside LineOS
❌ Pure DSP or algorithmic code placed in Aether (ML Origin Rule §05)
❌ Raw ML output crossing the Adapter Boundary unvalidated
❌ Sharing Rust types across the Elixir/Rust boundary
❌ CI checks that can be bypassed without documented exception
❌ Repository structure deviating from §10 without amendment
❌ An application shipping without a locked constitution
❌ Still Air implementing AV timeline editing (belongs to MotionCraft A2)
❌ Aether importing LineOS internal modules
❌ LineOS importing Aether
❌ Marketplace engines loading without M0 signature verification
❌ Using a forbidden technology or dependency (see §08.3)
❌ Apps defining step ordering or execution sequence (belongs to pipelines/)
❌ Aether modules orchestrating pipeline execution
❌ LineOS modules calling each other directly outside pipeline invocation
❌ Transformation logic or ML inference inside pipeline DAG definitions
```

---

## §14 — Amendment Process

1. Draft the amendment as a new section or separate file
2. Increment the version (`2.5 → 2.6`)
3. Document the reason and impact in the Changelog
4. Update all affected layer constitutions to reference the new version
5. Update CI to enforce any new rules

**Rules:**
- No retroactive amendments — once a version is locked, it is permanent
- Amendments are additive — existing sections may be clarified but not deleted
- Amendments require explicit approval from the Lead Architect
- An amendment that has not been merged into this document does not exist

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 2.6 | 2026-04-09 | Amendment A-001: §03 Execution Authority Model added. Pipelines are the sole runtime orchestration authority. Apps trigger, Aether enriches, LineOS executes, Pipelines sequence. CI gates: pipeline-logic-check.sh + layer-orchestration-check.sh. §13 Forbidden Work expanded with execution model violations. All subsequent sections renumbered. |
| 2.5 | 2026-04-09 | Clean rewrite. §01.3 Core Invariants. §03.1 Engine & Device Registry — namespace split: E1–E10 Aether devices, E11–E19 LineOS deterministic engines. E7=Link2Suno, E11=sp314-dsp, E12/E13 LineOS. §04 ML Origin Rule as constitutional section. §07.3 Forbidden Technologies consolidated. §11 ml-origin-check.sh added. Marketplace as peer layer throughout. |
| 2.4 | 2026-04-03 | Four-layer stack, Aether, AV layer, Elixir gateway, application registry A1–A6 |
| 2.3 | 2026-03-15 | Previous version (pre-Aether, pre-AV) |
| 1.1 | 2026-03-15 | Initial constitution |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `creator-os/constitution/creator-os-constitution.md`
**Version:** 2.6
**Date:** 2026-04-09
**Status:** 🔒 LOCKED

---

*If it is not in a constitution, it does not exist.*
*If it is not in a contract, it is not an interface.*
*If it bypasses M0, it is a security violation.*
*If it crosses the Adapter Boundary unvalidated, it is a determinism violation.*
*If it uses ML weights, it belongs in Aether.*
