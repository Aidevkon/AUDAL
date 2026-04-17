# LineOS — Phase 12A Master Prompt

**Document:** `lineos/plan/phase-12a/master-prompt.md`
**Version:** 1.0
**Phase:** 12A — xaak.rs PCM Kernel + cpal
**Tag:** `v0.12a.0-xaak`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6 · Amendment A-003
**Date:** 2026-04-17

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.11.0-dioxus
pwd && git branch --show-current
git status --short
just ci
bash infra/ci/checks/ui-isolation-check.sh
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat creator-os/constitution/amendments/A-003-audio-spine.md  # §1–§4 — binding
cat lineos/plan/phase-12a/task-decomposition.md
```

**A-003 §3 Phase Activation Rules are binding:**
- xaak.rs activates NOW (Phase 12)
- cpal activates NOW (Phase 12)
- Xaak-rs + sendspin = Phase 14 only
- No PCM in SessionStateJson, Cockpit, or any IPC struct

---

## 🎯 Phase 12A Goal

Implement the internal PCM kernel (`xaak.rs`) with lock-free
ring buffer and wire cpal as the I/O backend.

**Phase 12A delivers (backend only — no UI changes):**
- `xaak.rs` — lock-free ring buffer PCM kernel in LineOS M1
- `cpal` — I/O backend under xaak.rs
- Golden Blob PCM → xaak.rs → cpal → audio device
- Playback API: `play()`, `pause()`, `stop()`, `seek(ms)`
- New Tauri commands: `playback_control`, `get_playback_state`
- No Cockpit changes (Phase 12B)

**Phase 12B (next):**
- Transport bar UI wiring
- Live telemetry during playback
- Spectrum + CorrelationRadar canvas rendering

---

## 🔒 Forbidden in Phase 12A

```
❌ PCM in SessionStateJson (A-003 §2)
❌ PCM crossing Tauri IPC (A-003 §2)
❌ PCM in Cockpit components (A-003 §5)
❌ Xaak-rs or sendspin (Phase 14 only — A-003 §9)
❌ Arc<RwLock<Vec<f32>>> as PCM ownership (A-003 §4)
❌ mmap (Phase 14 only)
❌ Modifying sp314-dsp pipeline
❌ Modifying Dioxus Cockpit components
```

---

## 🏁 Exit Criteria

```bash
# xaak.rs compiles
cargo check -p xaak

# cpal device enumeration works
cargo test -p xaak -- --nocapture 2>&1 | grep "audio device"

# Playback Tauri commands registered
grep "playback_control\|get_playback_state" \
  apps/stillair/src-tauri/src/lib.rs

# UI isolation still clean
bash infra/ci/checks/ui-isolation-check.sh

cargo test --workspace
just ci
```

Commit + tag `v0.12a.0-xaak`.

---

**Lead Architect:** Anestis
**System:** LineOS M1
**Phase:** 12A
**Status:** 🔒 LOCKED


---