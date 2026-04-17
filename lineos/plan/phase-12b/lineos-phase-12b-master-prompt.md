# LineOS — Phase 12B Master Prompt

**Document:** `lineos/plan/phase-12b/master-prompt.md`
**Version:** 1.0
**Phase:** 12B — Transport Bar UI + Live Telemetry
**Tag:** `v0.12b.0-transport`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6 · Amendment A-003
**Date:** 2026-04-17

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.12a.0-xaak
pwd && git branch --show-current
just ci
bash infra/ci/checks/ui-isolation-check.sh
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat creator-os/constitution/amendments/A-003-audio-spine.md  # §5 UI isolation
cat apps/stillair/cockpit/design-system/design-tokens-v1.0.md  # §12 tokens
cat apps/stillair/cockpit/state-machine.md                    # transport states
cat lineos/plan/phase-12b/task-decomposition.md
```

---

## 🎯 Phase 12B Goal

Wire the Transport Bar UI in the Dioxus Cockpit to the
xaak.rs playback engine via Tauri IPC.

**Phase 12B delivers:**
- Transport bar: Play/Pause/Stop/Seek functional
- Live position display (polling every 500ms)
- Live momentary LUFS during playback (from telemetry)
- Spectrum analyzer — static bars replaced with real data
- CorrelationRadar — static placeholder → real stereo field
- Smoke test: play gargar.mp3 → hear audio from speakers

**Phase 12B does NOT deliver:**
- Xaak-rs / sendspin (Phase 14)
- Multi-room audio
- Cloud rendering

---

## 🔒 Forbidden in Phase 12B

```
❌ PCM in Cockpit components (A-003 §5)
❌ Direct xaak.rs import in Dioxus (ui-isolation enforced)
❌ Polling faster than 200ms (performance)
❌ Canvas WebGL (use SVG or HTML canvas only)
❌ Modifying xaak.rs or cpal (Phase 12A locked)
❌ Xaak-rs / sendspin (Phase 14)
```

---

## 🏁 Exit Criteria

```bash
# Manual: play audio
# Load gargar.mp3 → MASTER → FM5 → click PLAY
# → audio plays from speakers ✅
# → position counter increments ✅
# → PAUSE stops audio ✅
# → STOP resets position ✅

# Spectrum shows real bars (not placeholder)
# Correlation radar shows stereo field

bash infra/ci/checks/ui-isolation-check.sh
cargo test --workspace
just ci
```

Commit + tag `v0.12b.0-transport`.

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 12B
**Status:** 🔒 LOCKED


---