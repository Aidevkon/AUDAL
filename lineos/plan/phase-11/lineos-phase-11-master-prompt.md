# LineOS — Phase 11 Master Prompt

**Document:** `lineos/plan/phase-11/master-prompt.md`
**Version:** 1.0
**Phase:** 11 — Dioxus Cockpit Migration
**Tag:** `v0.11.0-dioxus`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6 · Amendment A-002
**Date:** 2026-04-16

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.10.0-export
pwd && git branch --show-current
git status --short
cargo check -p stillair
bash infra/ci/checks/ui-isolation-check.sh
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat creator-os/constitution/amendments/A-002-ui-disposable.md  # §7 migration path
cat apps/stillair/cockpit/state-machine.md                     # FSM spec — same logic
cat apps/stillair/src-tauri/src/commands/session.rs            # get_session_state
cat lineos/plan/phase-11/task-decomposition.md
```

---

## 🎯 Phase 11 Goal

Migrate the Still Air Cockpit from Leptos 0.7 to Dioxus 0.6,
following Amendment A-002 §7 migration path exactly.

**Migration sequence (binding — per A-002 §7):**
```
1. Write Dioxus Cockpit alongside existing Leptos Cockpit
2. Verify all Tauri IPC commands work from Dioxus
3. Verify all metrics display correctly (LRA, LUFS, Coach)
4. Switch Tauri window to Dioxus surface
5. Remove Leptos Cockpit code
6. Commit + tag v0.11.0-dioxus
```

**At no point may core components be modified.**
If a migration step requires touching DSP, Aether, or adapter-runtime → STOP.

**Framework:** Dioxus 0.6 stable (desktop feature)
**IPC:** Same Tauri commands — `triggerMastering`, `getSessionState`, `exportAudio`
**State:** CockpitMode FSM — same states, same transitions, Dioxus signals
**Entry point:** `get_session_state` (P9-008) replaces 3-step Data Cascade

---

## 🔒 Forbidden in Phase 11

```
❌ Modifying any core crate (sp314-dsp, telemetry, rule-engine, M0, Aether)
❌ Adding new Tauri commands (reuse existing)
❌ Changing GoldenBlobJson, SessionStateJson, CoachNarrativeJson schemas
❌ Leptos imports in Dioxus components
❌ Dioxus imports in core crates (ui-isolation-check.sh enforces)
❌ Removing Leptos until Dioxus is verified working end-to-end
❌ Business logic in Dioxus components (IPC only)
```

---

## 🏁 Exit Criteria

```bash
# Dioxus window opens
# Full flow: LOAD NEW → gargar.mp3 → Spotify → MASTER → FM5
# FM5 shows: -7.7 LUFS, 3.7 LU LRA, Coach narrative
# EXPORT works → mastered.flac written

# ui-isolation-check.sh still passes
bash infra/ci/checks/ui-isolation-check.sh

# Leptos code removed
ls apps/stillair/frontend/src/  # should be empty or removed

cargo test --workspace
just ci
```

Commit + tag `v0.11.0-dioxus`.

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 11 — Dioxus Cockpit Migration
**Status:** 🔒 LOCKED


---