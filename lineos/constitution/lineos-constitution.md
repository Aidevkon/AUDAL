# LineOS — Constitution

**Document:** `lineos/constitution/lineos-constitution.md`
**Version:** 2.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.5 — deterministic substrate layer
**Inherits:** `creator-os/invariants/creator-os-invariants.md` v1.1
**Supersedes:** LineOS Constitution v1.1

---

## Preamble

This document is the constitution of LineOS.

LineOS is the deterministic execution substrate of Creator OS. It enforces
the laws defined by the Creator OS Constitution. It has no knowledge of
Aether, no knowledge of ML computation, and no knowledge of the meta-OS
structure above it. It receives only validated, typed inputs and produces
only deterministic, schema-compliant outputs.

If any module constitution conflicts with this document, this document wins.
If this document conflicts with the Creator OS Constitution, the Creator OS
Constitution wins. If it conflicts with `creator-os-invariants.md`, the
invariants win.

---

## §01 — What LineOS Is

LineOS is the **deterministic execution substrate** of Creator OS.

It provides:
- **M0** — trust boundary, local CDN, reverse proxy, marketplace gatekeeper
- **M1 audio** — DSP engine (sp314-dsp), telemetry, metadata, insights, rule-engine
- **M1 AV** — av-core for deterministic AV processing and Golden Blob generation
- **WASM engines** — E12 (VideoEngine), E13 (AV Forge) served via M0
- **Golden Blob** — the canonical output artifact for both audio and AV

**One sentence:** LineOS enforces the laws, runs the computation, and produces
the output — deterministically, offline, and securely.

### §01.1 — What LineOS Is Not

- ❌ A cloud-first platform — cloud is opt-in, gated through M0
- ❌ A general-purpose OS — LineOS is domain-specific: audio and AV mastering
- ❌ An ML runtime — all ML computation belongs to Aether
- ❌ A probabilistic system — stochastic computation is forbidden in LineOS
- ❌ A multitrack DAW — Still Air previews AV; MotionCraft (A2) edits AV
- ❌ A knowledge of Creator OS structure — LineOS depends only on contracts

### §01.2 — Relationship to Creator OS

LineOS is the Layer 2 substrate of Creator OS. It depends on:
- `creator-os/contracts/` — JSON schema contracts (the only dependency upward)
- `creator-os/invariants/` — non-negotiable system invariants

LineOS does not import from Aether. LineOS does not import from Apps.
LineOS does not have knowledge of the constitutional layer above it.

---

## §02 — Module Registry

| Module | Role | Boundary | Layer |
|--------|------|----------|-------|
| M0 | Trust boundary, CDN, proxy, marketplace gatekeeper | Host native | M0 |
| sp314-dsp | 8-stage audio mastering engine — immutable | WASM + native | M1 audio |
| av-core | AV timeline orchestration and Golden Blob generation | Native | M1 AV |
| telemetry | EBU R128 Levels 1–9 + video metrics | Pod service | M1 |
| metadata | Report generation from Golden Blob | Pod service | M1 |
| insights | BMR-128 + EBU compliance comparator | Pod service | M1 |
| rule-engine | Deterministic compliance rule evaluator | Pod service | M1 |
| M1.6 sync | Opt-in cloud bridge (via M0 policy) | Pod service | M1 |
| E12 VideoEngine | Deterministic frame operations | WASM engine | Engines |
| E13 AV Forge | AV composition + sync | WASM engine | Engines |

**Rules:**
- No new module may be added without a constitution amendment
- Module IDs and roles are immutable once locked
- All WASM engines are served via M0 CDN — never accessed directly

---

## §03 — System Layers

Dependencies flow strictly downward. No layer imports from above.

```
Creator OS contracts  ← LineOS depends on these only
        ↓
       M0             ← trust boundary, all traffic passes through
        ↓
    M1 audio          ← sp314-dsp → telemetry → metadata → insights → rule-engine
    M1 AV             ← av-core (orchestrates E12, E13 via M0 IPC)
        ↓
    Cockpit (Apps)    ← communicates with LineOS via M0 only
```

---

## §04 — M0 (Trust Boundary)

M0 is the first process to start and the last to stop. See the authoritative
specification: `lineos/m0/constitution/m0-constitution.md`

**Key rules (summary):**
- All traffic between Apps/Aether and M1 services routes through M0
- Caddy binds only to `127.0.0.1` — never to an external interface
- Podman pods must not use `--network=host`
- M0 is the sole marketplace gatekeeper — all E100+ engines verified by M0
- `lineos/m0/api/m0-api.schema.json` is the sole public M0 interface

---

## §05 — M1 Audio (sp314-dsp)

The sp314-dsp engine is **immutable between phase releases**. It is the
single source of DSP truth in LineOS.

### §05.1 — Immutability Rules

- sp314-dsp source is frozen between phase releases
- The Pipeline Spec is frozen between phase releases
- Preset definitions (spotify, youtube, apple_music, tidal, raw) are frozen
- Any change to DSP logic, pipeline structure, or preset thresholds requires:
  a constitution amendment + an ADR + a new phase release

### §05.2 — Execution Modes

| Mode | Target | Consumer |
|------|--------|----------|
| WASM | `wasm32-unknown-unknown` | Cockpit (via M0 CDN) |
| Native | `x86_64-unknown-linux-gnu` | Pod services / CI |

Both modes must produce **identical binary output** for identical inputs and seeds.
Determinism is verified by automated test on every tagged release.

### §05.3 — Golden Blob (Audio)

sp314-dsp produces the canonical audio output artifact:

```rust
pub struct GoldenBlob {
    pub blob_type: BlobType,        // BlobType::Audio
    pub flac_bytes: Vec<u8>,
    pub quality_metrics: QualityMetrics,
    pub pipeline_params: PipelineParams,
    pub seed: u64,
    pub input_hash: [u8; 32],
}
```

All downstream modules read from the Golden Blob. No module re-processes
or re-measures audio. The Golden Blob is immutable once written.

---

## §06 — M1 AV (av-core)

av-core is the deterministic AV subsystem. It orchestrates E12 and E13
via M0 IPC and produces the Golden Blob (AV type).

### §06.1 — AV IPC Batching Rule (Binding)

av-core communicates with E12 (VideoEngine) and E13 (AV Forge) via M0 IPC.
**All video engine dispatch must use batch dispatch** — multiple frames per
IPC call. Per-frame IPC calls are forbidden. This is a performance invariant.

Shared memory (mmap) is a Phase 3 option for high-frequency paths.

### §06.2 — AV Golden Blob

av-core produces:

```rust
pub struct GoldenBlob {
    pub blob_type: BlobType,        // BlobType::Av
    pub video_frames: ContentAddressableChunks,
    pub audio_track: Vec<u8>,
    pub timeline_metadata: AvTimeline,
    pub quality_metrics: AvQualityMetrics,
    pub seed: u64,
    pub input_hash: [u8; 32],
}
```

The AV Golden Blob may be large. Content-addressable chunking is permitted
for storage. The contract (`creator-os/contracts/golden-blob.schema.json`)
remains the single source of truth for structure.

### §06.3 — AV Boundary (Constitutional)

Still Air (A1) **previews** AV — read-only playback, meters, export.
MotionCraft (A2) **edits** AV — timeline editing, cuts, composition.

This boundary is constitutional. Still Air implementing AV timeline editing
is a constitutional violation.

---

## §07 — M1 Services

All M1 services run inside the Podman pod. They are accessible only via M0.

| Service | Role | Rule |
|---------|------|------|
| telemetry | EBU R128 + video metrics | Reads Golden Blob only — never re-measures |
| metadata | Report generation | Reads Golden Blob only — never re-measures |
| insights | BMR-128 + EBU compliance | Comparator only — reads QualityMetrics |
| rule-engine | Deterministic rules | No LLM, no Aether, no randomness |
| M1.6 sync | Cloud bridge | Opt-in, policy-gated, never syncs raw audio |

**The comparator rule:** M1.3 (metadata) and M1.4 (insights) are comparators.
They read existing measurements from the Golden Blob. They never re-measure.
Any code path that reads raw audio in these modules is a constitution violation.

---

## §08 — Aether Boundary

LineOS is a deterministic system. Aether is stochastic. The boundary between
them is enforced by the Adapter Boundary:

```
Aether (stochastic)
    │  validate_output() — schema validation
    ▼
[ADAPTER BOUNDARY]  ← governed by feature-vector.schema.json
    │  typed, schema-compliant feature vectors only
    ▼
LineOS (deterministic)
```

**LineOS never imports Aether.** LineOS receives only validated feature vectors
through the Adapter Boundary. Raw ML output entering LineOS is a
constitutional violation.

The rule-engine is deterministic. It is not Aether. It evaluates compliance
rules — it does not produce stochastic output.

---

## §09 — Technology Constraints

### §09.1 — Math Library

`libm` is required for all floating-point math in deterministic processing paths.
`std::f32` and `std::f64` methods are forbidden in pipeline code.

### §09.2 — Determinism

Seeds must be explicit and derived from input data. `rand::thread_rng()` and
`OsRng` are forbidden in production pipelines.

Same input + same seed → identical binary output, always.
Determinism is verified by automated test: pipeline run ×2, binary diff = 0.

### §09.3 — Serialization

`serde_wasm_bindgen` is required at the WASM boundary.
`serde_json` is permitted in native crates (m0d, M1.x pod services).
`serde_json::Value` as Engine input type is forbidden — typed structs only.

### §09.4 — Pure Rust Constraint

LineOS is pure Rust. No C FFI, no C++ dependencies, no external subprocesses
except those mediated by M0.

---

## §10 — Deployment

### §10.1 — Container Model

- M1 audio and AV services run inside a rootless Podman pod
- M0 runs on the host (trust boundary — not containerised)
- Quadlet files in `infra/quadlet/` are the canonical deployment definition
- systemd supervises the M0 daemon

### §10.2 — Startup Order (Binding)

```
1. M0 reaches healthy status
2. M0 writes /run/lineos/m0-healthy
3. systemd/Quadlet starts Podman pod
4. M1.x services start
5. Apps become operational
```

No step may begin before the previous is complete.

### §10.3 — Deployment Rules

- M0 must be healthy before the pod starts — invariant, not a guideline
- All container images are pinned by digest in `m0-registry.json`
- No container runs as root — rootless Podman required
- Pod services are not directly accessible from host — all access via M0

---

## §11 — CI Gates

| Gate | Check |
|------|-------|
| Schema validation | `validate-schemas.sh` — zero errors |
| Determinism | Pipeline ×2, binary diff = 0 |
| WASM boundary | Zero `engine_wasm` imports in `cockpit/app/src/ui/` |
| ML origin | No ML weights in LineOS code paths |
| Dependency audit | `cargo deny check` — zero violations |
| Invariant inheritance | All modules declare `inherits_from: creator-os-invariants` |
| Container digest | All images pinned by digest |
| Audit log integrity | M0 audit NDJSON validates against schema |
| Direct pod access | M1.x pod ports connection refused from host |
| Podman network | No `--network=host` in container definitions |
| Health gate | All M0 §04.2 criteria pass within 30s |
| AV IPC | No per-frame IPC calls to E12/E13 |

---

## §12 — Forbidden Work (LineOS-Level)

```
❌ Any module performing DSP other than sp314-dsp
❌ Any module re-measuring audio (telemetry, metadata, insights are comparators)
❌ Raw ML output entering LineOS without passing the Adapter Boundary
❌ LineOS importing Aether
❌ Still Air implementing AV timeline editing (belongs to MotionCraft A2)
❌ Cloud sync enabled by default — explicit user action required
❌ M1.6 making outbound connections without M0 policy clearance
❌ Cockpit components importing engine internals directly
❌ Raw audio synced to cloud without explicit user consent
❌ Non-deterministic seeds in production pipelines
❌ Container services accessible without M0 proxy
❌ BMR-128 thresholds hardcoded — always read from bmr-128.schema.json
❌ Per-frame IPC calls to E12/E13 (AV batch dispatch required)
❌ std::f32 / std::f64 methods in DSP pipeline code
❌ serde_json::Value as Engine input type
❌ C FFI or C++ dependencies anywhere in LineOS
```

---

## §13 — Amendment Process

1. Draft the amendment
2. Increment version (`2.0 → 2.1`)
3. Changelog entry with reason and impact
4. If Creator OS-level rules are affected, update Creator OS Constitution
5. CI must enforce new rules

Amendments are additive. No section may be deleted.
Lead Architect approval required.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 2.0 | 2026-04-09 | Full rewrite aligned with Creator OS Constitution v2.5 and current architecture. LineOS redefined as deterministic substrate (not standalone OS). M1 AV section added (av-core, AV Golden Blob, AV boundary rule). Aether Boundary section added. AV IPC batching rule added as performance invariant. M0 section references m0-constitution.md as authoritative. Technology constraints consolidated. Supersedes v1.1. |
| 1.1 | 2026-03-26 | Pipeline Spec + preset immutability; sync payload clarified; visualization-only metrics rule |
| 1.0 | 2026-03-26 | Initial constitution |

---

**Lead Architect:** Anestis
**System:** LineOS
**Document:** `lineos/constitution/lineos-constitution.md`
**Version:** 2.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED

---

*This is the law of the substrate.*
*Build from this. Deviate from nothing.*
*If it is not in a constitution, it does not exist.*
