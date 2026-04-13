# Creator OS — Amendment A-001: Execution Authority Model

**Document:** `creator-os/constitution/amendments/A-001-execution-model.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Amends:** Creator OS Constitution v2.5 → v2.6
**Approved by:** Lead Architect — Anestis

---

## Context

The Creator OS Constitution v2.5 defines what each layer **is** and what
dependencies are permitted. It does not explicitly define **who has
orchestration authority at runtime** — i.e., who decides what happens next,
in what order, and across which boundaries.

Without an explicit execution authority model, the following drift patterns
emerge:
- Apps implementing business logic or step sequencing
- Aether adapters orchestrating pipeline execution
- LineOS modules defining their own invocation order
- Duplicated flows across layers

This amendment closes that gap.

---

## Amendment Text

### §X — Execution Authority Model

Add as new section after §02 in Creator OS Constitution v2.5.

---

#### §X.1 — Execution Authority Belongs to Pipelines

Runtime orchestration authority belongs **exclusively** to `pipelines/`.

No other layer may define step ordering, data routing between layers,
or execution sequence. This is a constitutional rule, not a guideline.

```
User intent (Apps)
    │  triggers
    ▼
Pipeline DAG (pipelines/)   ← sole execution authority
    │  sequences
    ├── LineOS step (execute)
    ├── Aether step (enrich)
    └── LineOS step (execute)
         │
         ▼
    Output → App
```

#### §X.2 — Layer Roles Under This Model

| Layer | Runtime role | What it must NOT do |
|-------|-------------|---------------------|
| Apps | Express user intent, trigger pipelines, render output | Define execution sequence or business logic |
| Aether | Enrich steps with ML when invoked by a pipeline | Orchestrate execution or call LineOS directly |
| LineOS | Execute individual steps deterministically | Define its own invocation order |
| Pipelines | Define step ordering, data routing, boundary crossings | Contain transformation logic or ML inference |

#### §X.3 — Pipeline Definition Rules

Pipelines are **declarative DAGs only**. They are defined as JSON/YAML files.
No Rust transformation code lives in the Pipeline layer.

```
✅ Permitted in a pipeline definition:
   - Step ordering (A → B → C)
   - Routing decisions (if metric > threshold → branch)
   - Parameter passing between steps
   - Boundary crossing declarations (Aether step → LineOS step)

❌ Forbidden in a pipeline definition:
   - Audio or video transformation logic
   - ML inference
   - Business logic
   - Direct M0 calls
   - Schema validation (belongs to the step, not the pipeline)
```

**Definitions (for CI enforcement):**
- *Business logic* = any transformation of data beyond routing or parameter passing
- *Transformation logic* = any modification of input values not explicitly defined
  in a contract field mapping

CI must detect non-trivial data mutation inside Pipeline definitions.

#### §X.4 — Trigger Authority

Only Apps may trigger pipelines. No pipeline may trigger another pipeline
directly — fan-out must go through an App or a parent DAG node.

Aether devices and LineOS modules are **steps** inside pipelines — they are
invoked by the pipeline runner, never by each other.

#### §X.5 — Enforcement

- `pipeline-logic-check.sh` — CI must verify no transformation logic in pipeline DAG files
- `layer-orchestration-check.sh` — CI must verify no App, Aether module, or LineOS module
  contains step-sequencing logic outside `pipelines/`
- Pipeline DAG schemas must be validated against `dag.schema.json` on every commit

---

## Impact on Existing Documents

| Document | Change required |
|----------|----------------|
| Creator OS Constitution | Add §X (this amendment merges as §03 in v2.6) |
| ARCHITECTURE.md | Add execution model diagram to `pipelines/` section |
| Pipelines Constitution v1.0 | New document — see companion doc |
| LineOS Constitution | No change — §03 already states strict downward dependencies |
| Aether Constitution | Add note: Aether adapters are pipeline steps, not orchestrators |

---

## What This Resolves

| Problem | Resolution |
|---------|-----------|
| "Who decides sequence?" | Pipelines — exclusively |
| "Can Aether orchestrate?" | No — Aether is a step, not an orchestrator |
| "Can Apps define execution logic?" | No — Apps express intent and trigger pipelines |
| "Can LineOS modules call each other?" | No — invocation order is the pipeline's responsibility |

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-04-09 | Initial amendment — Execution Authority Model |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Amendment:** A-001
**Status:** 🔒 LOCKED
