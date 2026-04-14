# LineOS — Phase 5 Master Prompt

**Document:** `lineos/plan/phase-5/master-prompt.md`
**Version:** 1.0
**Phase:** 5 — Cockpit (Still Air UI)
**Tag:** `v0.5.0-cockpit`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6
**Date:** 2026-04-14

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.4.0-rule-engine
pwd && git branch --show-current
git status --short
rustup target list --installed | grep wasm32-unknown-unknown
trunk --version
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat apps/stillair/cockpit/state-machine.md   # FSM spec — read FIRST
cat lineos/constitution/lineos-constitution.md
cat lineos/architecture/lineos-architecture.md
cat lineos/plan/phase-5/task-decomposition.md
```

**The state machine spec is the primary authority for this phase.**
Every UI decision must be consistent with it.

---

## 🎯 Phase 5 Goal

Build the navigable Still Air Cockpit — a Tauri 2.x + Leptos application
with the FM0–FM6 state machine, 3 MFD panels, and the core mastering
flow functional end-to-end.

**Core mastering flow (must be functional):**
```
FM0 → file drop → FM1 → preset select → FM1.5 →
master click → FM2 (progress) → FM3 → FM4 → FM5
(CoachFindings visible)
```

**Non-core features (visually complete placeholders):**
- FM6 Export — button visible, shows "Export coming in Phase 6"
- Telemetry live meters during FM2 — static progress bar acceptable
- Coach narrative — findings shown as structured list (no LLM)

At end of phase:
- `apps/stillair/` Tauri app builds and runs
- State machine enforced (typestate + CockpitMode enum)
- 3 MFD panels render correctly per FM
- Core mastering flow works end-to-end
- `just ci` passes
- Tag `v0.5.0-cockpit`

---

## 🔒 Forbidden in Phase 5

```
❌ Business logic or rule evaluation in Leptos components
❌ Direct audio buffer access from UI components
❌ Polling M0 for telemetry — subscribe to events only
❌ Hot-swapping audio files without hard_reset()
❌ FM1.5 → FM2 transition without a valid sealed Intent
❌ FM-ERR → FM1 shortcut (must go through FM0)
❌ LLM calls (Narrative Layer is Aether — Phase 7+)
❌ serde_json::Value crossing WASM boundary
❌ Tauri blocking commands for audio processing
❌ Any UI component importing lineos-rule-engine directly
```

---

## 🏁 Exit Criteria

```bash
# Tauri app builds
cd apps/stillair && cargo tauri build
# Expected: success

# Frontend WASM compiles
cargo check --target wasm32-unknown-unknown -p stillair-frontend
# Expected: 0 errors

# All tests pass
cargo test --workspace
# Expected: all pass

# CI gates
just ci
# Expected: ✅

# Manual: core flow
# FM0 → drop file → FM1 → select preset → FM1.5 → click master
# → FM2 (progress) → FM3 → FM4 → FM5 (findings visible)
```

Commit + tag `v0.5.0-cockpit`.

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 5 — Cockpit
**Status:** 🔒 LOCKED
