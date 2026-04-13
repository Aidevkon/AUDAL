# Creator OS — Ignition Prompt

You are starting work on **Creator OS** — a governed, deterministic,
local-first operating system for audio, video, and image production.

Your workspace is at:
```
/home/documents/creator-os/
```

This is the git repo root. The `canonical` branch is active.

---

## Step 1 — Read the bootstrap FIRST

```bash
cat docs/bootstrap.md
```

Read it completely. It defines pre-conditions, reading order, absolute
rules, and phase execution protocol.

**Do not proceed until every pre-condition passes.**

---

## Step 2 — Verify pre-conditions

```bash
pwd
# Expected: /home/documents/creator-os

git branch --show-current
# Expected: canonical

rustc --version && cargo --version
rustup target list --installed | grep wasm32-unknown-unknown
wasm-opt --version
trunk --version
podman --version && podman info | grep -i rootless
just --version
cargo watch --version
b3sum --version
```

Report format:
```
✅ Pre-conditions:
- pwd: /home/documents/creator-os ✅
- branch: canonical ✅
- rustc: [version] ✅
- wasm32-unknown-unknown: installed ✅
- wasm-opt: [version] ✅
- trunk: [version] ✅
- podman: [version] rootless: yes ✅
- just: [version] ✅
- cargo-watch: [version] ✅
- b3sum: [version] ✅
```

Any failure → STOP. Report. Do not proceed.

---

## Step 3 — Read governing documents

Read in this exact order:

```bash
# 1. Highest authority
cat creator-os/invariants/creator-os-invariants.md

# 2. Creator OS Constitution
cat creator-os/constitution/creator-os-constitution.md

# 3. ARCHITECTURE
cat ARCHITECTURE.md

# 4. LineOS Constitution
cat lineos/constitution/lineos-constitution.md

# 5. LineOS Architecture
cat lineos/architecture/lineos-architecture.md

# 6. M0 Constitution (PRIMARY for Phase 1)
cat lineos/m0/constitution/m0-constitution.md

# 7. LineOS PRD
cat lineos/prd/lineos-prd.md

# 8. ADR
cat lineos/architecture/adr.md

# 9. Aether Constitution
cat aether/constitution/aether-constitution.md

# 10. Pipelines Constitution
cat pipelines/constitution/pipelines-constitution.md

# 11. Marketplace Constitution
cat marketplace/constitution/marketplace-constitution.md
```

---

## Step 4 — Confirm readiness

```
✅ Documents read:
- creator-os-invariants v1.1 ✅
- creator-os-constitution v2.6 ✅
- ARCHITECTURE v3 ✅
- lineos-constitution v2.0 ✅
- lineos-architecture v2.0 ✅
- m0-constitution v2.0 ✅
- lineos-prd v2.0 ✅
- adr v1.0 ✅
- aether-constitution v1.0 ✅
- pipelines-constitution v1.0 ✅
- marketplace-constitution v1.0 ✅

Authority chain understood:
creator-os-invariants → creator-os-constitution → layer constitutions
→ module constitutions → phased plan → task decompositions

Absolute rules understood:
- No DSP outside lineos/m1/sp314-dsp ✅
- No ML weights in lineos/ ✅
- No raw ML output crossing Adapter Boundary ✅
- No engine_wasm imports in cockpit ui/ ✅
- Caddy binds 127.0.0.1 only ✅
- No --network=host ✅
- No runtime asset downloads ✅
- No rand::thread_rng() in production pipelines ✅
- No std::f32 methods in DSP — use libm ✅
- No serde_json::Value as Engine input ✅
- No hardcoded BMR-128 thresholds ✅
- No whisper-rs / ring (C FFI) ✅
- No per-frame IPC to E12/E13 ✅
- Pipelines are sole execution authority ✅

Ready for: Phase 1 — M0 Moat
```

---

## Step 5 — Begin Phase 1

```bash
cat lineos/plan/phase-1/master-prompt.md
cat lineos/plan/phase-1/task-decomposition.md
```

Execute tasks in order. Gate-before-proceed is non-negotiable.

```
P1-001  Workspace + m0d crate scaffold
P1-002  Registry + checksums (blake3 + SHA-256)
P1-003  Health gate + health endpoint (7 criteria)
P1-004  Caddy integration + reverse proxy
P1-005  Asset CDN + hash verification
P1-006  Audit subsystem
P1-007  Policy enforcement
P1-008  Marketplace gatekeeper (Ed25519 + blake3)
P1-009  m0-api.schema.json (API freeze boundary)
P1-010  systemd + Quadlet integration
P1-011  Justfile recipes
P1-012  Integration tests + DoD gate
```

---

## Authority Chain

```
creator-os-invariants        ← highest
        ↓
creator-os-constitution
        ↓
layer constitutions
(lineos / aether / pipelines / marketplace)
        ↓
module constitutions
(m0 / ...)
        ↓
phase master prompts
        ↓
task decompositions           ← lowest
```

Higher authority always wins. Report conflicts before proceeding.

---

**System:** Creator OS
**Repo:** `/home/documents/creator-os/`
**Branch:** `canonical`
**Lead Architect:** Anestis
**Start:** Phase 1 — M0 Moat → `v0.1.0-m0`
