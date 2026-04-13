# LineOS — Agent Bootstrap

**Document:** `lineos/bootstrap.md`
**Version:** 1.3
**Date:** 2026-04-09
**Status:** 🔒 READ THIS FIRST — before any phase begins
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.5
**Changes from v1.2:** All document references updated to v2.0 suite. serde_json::Value rule already present. Paths updated for new repo structure.
**Lead Architect:** Anestis

---

## ⚠️ PRE-CONDITIONS (Non-Negotiable)

```bash
# 1. Correct working directory
pwd
# Expected: .../LineOS (repo root)

# 2. Canonical branch, at least one commit
git branch --show-current
# Expected: canonical

# 3. Rust toolchain
rustc --version && cargo --version

# 4. WASM target
rustup target list --installed | grep wasm32-unknown-unknown
# Expected: wasm32-unknown-unknown (installed)

# 5. wasm-opt
wasm-opt --version

# 6. Trunk
trunk --version

# 7. Podman rootless
podman --version
podman info | grep -i "rootless"
# Expected: rootless: true

# 8. just
just --version

# 9. cargo-watch
cargo watch --version

# 10. b3sum (blake3 CLI — for asset digests)
b3sum --version
# If missing: cargo install b3sum
```

**If any pre-condition fails → STOP. Report which check failed.**

---

## 🖥️ Environment Notes (Informational — not blocking)

```bash
echo $TERM_PROGRAM
echo $WAYLAND_DISPLAY
# Expected on this setup: WezTerm on Wayland/AMD
# If different: note it, do not STOP
```

---

## 📖 Mandatory Reading Order

Read ALL before writing any code. These documents are law.

```bash
# 1. Highest authority
cat lineos/creator-os/invariants/creator-os-invariants.md

# 2. Creator OS Constitution
cat lineos/creator-os/constitution/creator-os-constitution-v2.5.md

# 3. LineOS Constitution
cat lineos/constitution/lineos-constitution.md

# 4. LineOS Architecture
cat lineos/architecture/lineos-architecture.md

# 5. M0 Constitution (PRIMARY for Phase 1)
cat lineos/m0/constitution/m0-constitution.md

# 6. PRD
cat lineos/prd/lineos-prd.md

# 7. Phased Plan
cat lineos/plan/lineos-phased-plan.md

# 8. ARCHITECTURE.md
cat ARCHITECTURE.md
```

**Conflict resolution:**
`creator-os-invariants` → `creator-os-constitution` → `lineos-constitution`
→ `module constitutions` → `phased plan` → `task decompositions`

Higher authority always wins. Report conflicts before proceeding.

---

## 🗂️ Repository Structure

```
LineOS/                              ← git root, here you run just
├── Justfile                         ← root recipes
├── ARCHITECTURE.md
├── Cargo.toml                       ← workspace root
├── creator-os/                      ← constitutional layer
│   ├── constitution/
│   ├── invariants/
│   ├── contracts/
│   └── shared/adapter-runtime/
├── lineos/                          ← deterministic OS core
│   ├── constitution/
│   ├── architecture/
│   ├── prd/
│   ├── plan/
│   ├── m0/                          ← trust boundary
│   │   ├── Justfile
│   │   ├── api/                     ← m0-api.schema.json (API freeze boundary)
│   │   ├── m0-daemon/
│   │   ├── sandbox/                 ← Phase 2+
│   │   ├── assets/wasm/             ← WASM engines (version-pinned)
│   │   ├── assets/engines/          ← native binaries
│   │   ├── registry/
│   │   ├── config/
│   │   ├── logs/
│   │   ├── docs/
│   │   └── systemd/
│   ├── m1/
│   │   ├── sp314-dsp/               ← immutable audio engine
│   │   ├── av-core/                 ← AV orchestration
│   │   ├── telemetry/
│   │   ├── metadata/
│   │   ├── insights/
│   │   ├── rule-engine/
│   │   └── m1.6-sync/
│   └── shared/schema/               ← LineOS-internal schemas
├── aether/                          ← ML enrichment layer
├── engines/                         ← WASM component engines
├── apps/stillair/                   ← Still Air (A1)
├── marketplace/                     ← ecosystem layer
├── infra/
│   ├── quadlet/
│   └── ci/checks/
└── scripts/
```

---

## 🔒 Absolute Rules (Apply to Every Phase)

```
❌ Never perform DSP outside sp314-dsp
❌ Never re-measure audio (telemetry/metadata/insights are comparators)
❌ Never allow ML weights into lineos/
❌ Never let raw ML output cross the Adapter Boundary unvalidated
❌ Never import engine internals in cockpit ui/ components
❌ Never allow direct App → M1 traffic (always via M0)
❌ Never bind Caddy to 0.0.0.0 — localhost only
❌ Never use --network=host for Podman pods
❌ Never download assets or engines at runtime
❌ Never enable cloud sync by default
❌ Never use rand::thread_rng() in production pipelines
❌ Never use std::f32::tanh() in DSP code — use libm
❌ Never use serde_json::Value as Engine input type
❌ Never define schema contracts outside creator-os/contracts/
❌ Never hardcode BMR-128 thresholds — read from bmr-128.schema.json
❌ Never use whisper-rs (C++ FFI) — use Candle Whisper
❌ Never use ring crate (C deps) — use ed25519-dalek + blake3
❌ Never use per-frame IPC to E12/E13 — batch dispatch only
```

---

## 📋 Phase Execution Protocol

```
1. READ   → phase master prompt
2. READ   → phase task decomposition
3. VERIFY → all pre-conditions
4. EXECUTE → tasks in order, gate-before-proceed
5. VERIFY → each task's DoD before next task
6. GATE   → run: just gate-phaseN
7. COMMIT → conventional commit message
8. TAG    → git tag vX.Y.Z-phaseName
9. REPORT → confirm tag + all gates passed
```

Gate-before-proceed is non-negotiable.

---

## 🏷️ Git Convention

```bash
# Conventional Commits
feat(m0): add m0d health endpoint
feat(m1-audio): implement stage 3 de-esser
fix(cockpit): remove engine_wasm import from session panel
test(m0): add marketplace gatekeeper tamper test
chore(infra): add m0d.quadlet systemd unit

# Phase tags
git tag v0.0.0-constitution
git tag v0.1.0-m0
git tag v0.2.0-dsp
git tag v0.3.0-telemetry
git tag v0.4.0-insights
git tag v0.5.0-cockpit
git tag v0.6.0-coach-sync
git tag v1.0.0-release
```

---

## 🔍 Universal CI Checks

```bash
# Full CI gate (run before every tag)
just ci

# M0-specific gate
just m0::ci

# Phase 1 integration gate
just gate-phase1

# Individual checks
just validate-schemas
just check-boundary
just check-network
just check-ml-origin
just check-thresholds
just m0::verify-assets
just m0::health
```

---

## 🧱 Technology Stack Reference

| Domain | Technology | Constraint |
|--------|-----------|-----------|
| DSP engine | Rust `no_std + alloc` | `libm` only — never `std::f32` |
| WASM target | `wasm32-unknown-unknown` + wasm-opt | → `m0/assets/wasm/` |
| Desktop shell | Tauri 2.x + Leptos | WASM boundary enforced |
| UI bundler | Trunk | → `m0/assets/cockpit/` |
| Reverse proxy | Caddy | `127.0.0.1` only |
| Container runtime | Podman rootless | No `--network=host` |
| Audio decoding | symphonia (MPL-2.0) | Covers all formats |
| Serialization (WASM) | serde_wasm_bindgen | Never serde_json at WASM boundary |
| Serialization (native) | serde_json | Permitted in m0d, M1.x services |
| Signing | ed25519-dalek + blake3 | Pure Rust — no ring |
| Asset digest | blake3 | Primary for registry |
| Asset checksum | SHA-256 | Secondary for checksums.json |

---

## ✅ Bootstrap Complete

```
✅ Bootstrap complete.
- Working directory: [path]
- Branch: canonical
- Toolchain: rustc [version], cargo [version]
- WASM target: installed
- wasm-opt: [version]
- Podman: [version], rootless: yes
- just: [version]
- cargo-watch: [version]
- b3sum: [version]
- Terminal: [TERM_PROGRAM or "not set"] (informational)
- Documents read: invariants · creator-os-constitution · lineos-constitution
                  lineos-architecture · m0-constitution · prd · phased-plan
                  ARCHITECTURE.md
- Ready for: Phase [N] — [phase name]
```

---

**Lead Architect:** Anestis
**System:** LineOS
**Document:** `lineos/bootstrap.md`
**Version:** 1.3
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
