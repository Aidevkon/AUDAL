# LineOS — Phase 6 Master Prompt

**Document:** `lineos/plan/phase-6/master-prompt.md`
**Version:** 1.0
**Phase:** 6 — Wire Everything (M0 IPC + Full Audio Flow)
**Tag:** `v0.6.0-wired`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6
**Date:** 2026-04-14

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.5.0-cockpit
pwd && git branch --show-current
git status --short
cargo check -p stillair
cargo check --target wasm32-unknown-unknown -p stillair-frontend
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat apps/stillair/cockpit/state-machine.md
cat apps/stillair/cockpit/design-system/design-tokens-v1.0.md
cat lineos/m0/constitution/m0-constitution.md     # §03 M0 IPC rules
cat lineos/m1/rule-engine/README.md
cat lineos/plan/phase-6/task-decomposition.md
```

---

## 🎯 Phase 6 Goal

Replace all Phase 5 stubs with real implementations.
Wire the complete audio flow end-to-end:

```
Cockpit (Leptos)
    │ Tauri IPC command
    ▼
Tauri backend (Rust)
    │ reqwest → localhost:7400
    ▼
M0 (running m0d)
    │ spawns / invokes
    ▼
sp314-dsp (native) → telemetry → insights → rule-engine
    │
    ▼
Golden Blob (JSON via Tauri IPC)
    │
    ▼
Cockpit: FM3 → FM4 → FM5 (Data Cascade with real data)
```

**Also in Phase 6:**
- CSS token cleanup: replace all inline hex with design token variables
- FM6 Export: real FLAC/WAV export via M0
- Live telemetry during FM2 (100ms events)

At end of phase:
- Full mastering flow works with real audio
- All CSS uses design tokens (no inline hex)
- M0 must be running for the app to function
- `just ci` passes
- Tag `v0.6.0-wired`

---

## 🔒 Forbidden in Phase 6

```
❌ Cockpit holding binary audio state (Golden Blob = JSON only)
❌ Tauri backend calling sp314-dsp directly (must go through M0)
❌ Polling M0 for telemetry — event subscription only
❌ Inline hex literals in CSS (use design token variables)
❌ serde_json::Value crossing WASM boundary
❌ M0 endpoints added without updating m0-api.schema.json
❌ Audio processing in Tauri backend
```

---

## 🏁 Exit Criteria

```bash
# No inline hex in CSS
grep -r "#[0-9a-fA-F]\{3,6\}" apps/stillair/frontend/src/
# Expected: 0 results

# Full build
cargo check -p stillair
cargo check --target wasm32-unknown-unknown -p stillair-frontend

# All tests
cargo test --workspace

# CI
just ci && just deny

# Manual: full flow with real audio
# Drop .wav file → select preset → MASTER →
# FM2 (real progress) → FM3 → FM4 → FM5 (real LUFS/TP visible)
```

Commit + tag `v0.6.0-wired`.

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 6
**Status:** 🔒 LOCKED


---