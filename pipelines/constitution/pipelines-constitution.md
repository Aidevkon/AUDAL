# Creator OS — Pipelines Constitution

**Document:** `pipelines/constitution/pipelines-constitution.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.5 · Amendment A-001
**Inherits:** `creator-os/invariants/creator-os-invariants.md` v1.1

---

## Preamble

This document is the constitution of the Pipelines layer.

Pipelines are the sole execution authority of Creator OS at runtime.
They decide what happens next — the sequence of steps, the routing of data,
and the crossing of layer boundaries. Nothing else does.

A Pipeline is a declarative DAG. It has no ML weights. It has no DSP code.
It sequences and routes. That is its only job.

If any rule in this document conflicts with the Creator OS Constitution or
Amendment A-001, those documents win.

---

## §01 — What Pipelines Are

The Pipelines layer provides **runtime orchestration** for Creator OS.

It provides:
- Declarative DAG definitions (JSON/YAML)
- A DAG runner that executes steps in dependency order
- A scheduler for parallel step execution where permitted
- A state machine for pipeline lifecycle management
- Routing logic between steps

**One sentence:** Pipelines define what happens, in what order, with what data —
and nothing else.

### §01.1 — What Pipelines Are Not

- ❌ A DSP engine — pipelines never process audio or video
- ❌ An ML runtime — pipelines never run inference
- ❌ A business logic layer — pipelines route and sequence; they do not transform
- ❌ A storage layer — pipelines hold no persistent state
- ❌ An App — pipelines have no UI and are never triggered by users directly
- ❌ An orchestrator of Apps — Apps trigger pipelines, not the reverse

---

## §02 — Execution Authority

Pipelines have exclusive runtime orchestration authority.

No App, Aether module, or LineOS module may define step ordering or
execution sequence outside `pipelines/`. This is a hard rule enforced by CI.

### §02.1 — What Each Layer Contributes

```
Apps        → express user intent → trigger pipeline
                                         │
                                    Pipeline DAG
                                    (sole authority)
                                         │
              ┌──────────────────────────┼────────────────────────┐
              │                          │                        │
         Aether step               LineOS step             LineOS step
         (ML enrichment)           (DSP execution)         (compliance)
              │                          │                        │
              └──────────────────────────┼────────────────────────┘
                                         │
                                    Output → App
```

### §02.2 — Trigger Rules

- Only Apps may trigger pipelines
- A pipeline may not trigger another pipeline directly
- Fan-out must go through an App or a parent DAG node
- Pipelines are invoked by Apps via `pipelineforge` public API only

---

## §03 — DAG Definition Rules

Pipeline DAGs are declarative. They are JSON or YAML files.
No Rust transformation code lives in `pipelines/`.

### §03.1 — Permitted in a DAG Definition

```yaml
# ✅ Step ordering
steps:
  - id: denoise
    type: aether_device
    engine: E4
    input: $.audio_input

  - id: master
    type: lineos_m1
    engine: E11
    input: $.denoise.output
    depends_on: [denoise]

# ✅ Conditional routing
  - id: av_branch
    type: router
    condition: "$.input.media_type == 'av'"
    if_true: av_pipeline
    if_false: audio_pipeline

# ✅ Parameter passing
  - id: telemetry
    type: lineos_m1
    input: $.master.golden_blob
    params:
      standard: "BS.1770-4"
```

### §03.2 — Forbidden in a DAG Definition

```
❌ Audio or video transformation logic
❌ ML inference or model calls
❌ Schema validation (belongs to the step, not the DAG)
❌ Direct M0 API calls
❌ Business logic (if user is premium → do X)
❌ Inline Rust or code execution
❌ Cross-pipeline imports
```

**CI enforcement:** `pipeline-logic-check.sh` scans all DAG files for
forbidden patterns. Non-trivial data mutation inside a DAG = build failure.

### §03.3 — DAG Schema Validation

All DAG files must validate against `pipelines/pipelineforge/schemas/dag.schema.json`.
An invalid DAG = build failure. A DAG without a schema companion = build failure.

---

## §04 — Pipeline Registry (Phase 1)

| Pipeline | File | Purpose |
|----------|------|---------|
| `stillair-mastering` | `stillair-mastering.dag.json` | Audio-only mastering (Still Air A1) |
| `stillair-av-mastering` | `stillair-av-mastering.dag.json` | AV mastering (Still Air A1) |

New pipelines require a constitution amendment.

---

## §05 — Step Types

A step in a DAG has a declared type. The type determines which layer executes it.

| Step type | Executed by | Example |
|-----------|------------|---------|
| `lineos_m1` | LineOS M1 service | sp314-dsp, telemetry, insights |
| `aether_device` | Aether device | E4 denoise, E7 link2suno |
| `aether_adapter` | Aether adapter | coach-adapter, feature-adapter |
| `router` | Pipeline runner | conditional branching |
| `merge` | Pipeline runner | fan-in after parallel steps |

Steps communicate via contract-validated data only.
No step may import another step's internal code.

---

## §06 — Determinism Contract

### §06.1 — Deterministic Steps

Steps declared as `deterministic: true` must produce identical output for
identical inputs and seeds. The pipeline runner verifies this.

### §06.2 — Stochastic Steps

Steps declared as `deterministic: false` (Aether devices/adapters) produce
stochastic output. Their output must pass schema validation before the next
step receives it.

A stochastic step may never feed directly into a deterministic step without
an explicit validation node between them.

### §06.3 — Pipeline Determinism

A pipeline is deterministic if and only if all its steps are deterministic.
A pipeline containing at least one Aether step is stochastic at the pipeline
level — but LineOS steps within it remain deterministic individually.

---

## §07 — Repository Structure (Canonical)

```
pipelines/
├── constitution/
│   └── pipelines-constitution.md      ← this document
└── pipelineforge/
    ├── Cargo.toml
    ├── src/
    │   ├── dag-runner.rs              ← executes DAG in dependency order
    │   ├── scheduler.rs               ← parallel step execution
    │   └── state-machine.rs           ← pipeline lifecycle (pending → running → complete → failed)
    ├── dags/
    │   ├── stillair-mastering.dag.json
    │   └── stillair-av-mastering.dag.json
    └── schemas/
        ├── dag.schema.json            ← validates all DAG files
        └── dag-av.schema.json         ← AV-specific DAG schema
```

**This structure is immutable.** Any deviation requires a constitution amendment.

---

## §08 — CI Gates

| Gate | Check |
|------|-------|
| DAG schema validation | All DAGs valid against `dag.schema.json` |
| Pipeline logic check | `pipeline-logic-check.sh` — no transformation logic in DAGs |
| Layer orchestration check | `layer-orchestration-check.sh` — no step-sequencing in Apps/Aether/LineOS |
| Step type validation | All step types declared and valid |
| No inline code | No Rust, Python, or script content in DAG files |

---

## §09 — Forbidden Work (Pipelines-Level)

```
❌ Pipelines processing audio, video, or any media data
❌ Pipelines running ML inference
❌ Pipelines containing business logic
❌ Pipelines calling M0 directly
❌ Pipelines triggering other pipelines directly
❌ Apps defining step ordering outside pipelines/
❌ Aether modules orchestrating execution
❌ LineOS modules calling each other directly
❌ A stochastic step feeding directly into a deterministic step without validation
❌ DAG files without a companion schema
❌ New pipelines added without a constitution amendment
```

---

## §10 — Amendment Process

1. Draft the amendment
2. Increment version (`1.0 → 1.1`)
3. Changelog entry
4. Update Creator OS Constitution if OS-level rules are affected
5. CI must enforce new rules

Amendments are additive. Lead Architect approval required.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-04-09 | Initial constitution — execution authority model, DAG-only rule, step types, determinism contract, CI gates |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `pipelines/constitution/pipelines-constitution.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED

---

*Pipelines decide what happens next.*
*Nothing else does.*
