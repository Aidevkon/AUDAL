# LineOS — Phase 14 Master Prompt

**Document:** `lineos/plan/phase-14/master-prompt.md`
**Version:** 1.0
**Phase:** 14 — UI Polish (Avionics Aesthetic)
**Tag:** `v0.14.0-ui`
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.6 · Amendment A-002 · UI Agent Context v2.1
**Date:** 2026-04-22

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.13.0-mp3
pwd && git branch --show-current
just ci
bash infra/ci/checks/ui-isolation-check.sh
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat apps/stillair/cockpit/ui-agent-context.md   # v2.1 — THE authority doc
cat creator-os/constitution/amendments/A-002-ui-disposable.md  # §3, §9
cat lineos/plan/phase-14/task-decomposition.md
```

**Also study the reference images before writing any CSS:**
- `apps/stillair/cockpit/design-system/mockups/insights-closeup.jpg`
- `apps/stillair/cockpit/design-system/mockups/vu-meters-closeup.jpg`
- `apps/stillair/cockpit/design-system/mockups/mastered-view.png`

---

## 🎯 Phase 14 Goal

Transform the functional Dioxus Cockpit into the avionics hardware
aesthetic shown in the reference mockups.

**Deliverables:**
- Panel containers with OLED inset + screws + correct shadow stack
- VU meters: segmented (30 LEDs, not smooth bars)
- SpectrumDisplay: SVG waveform curve (not bar chart)
- StereoScope: dual-ellipse Lissajous (cyan outer + magenta inner)
- MetricsReadout: PEAK/RANGE/CORR with correct colors + sizes
- CoachPanel: progress bars + severity dots + YES/NO buttons
- TransportBar: hardware button aesthetic, all clicks working
- MasteredView: BEFORE/AFTER waveform + Quality Gate + Compliance
- New Tauri command: `getVisualizationData` (backend computes SVG paths)

**At end of phase:**
- Cockpit matches reference mockups
- All transport bar buttons clickable
- `just ci` passes
- `ui-isolation-check.sh` passes
- Tag `v0.14.0-ui`

---

## 🔒 Forbidden in Phase 14

```
❌ SVG path computation in UI components
❌ Waveform downsampling in UI
❌ Business logic in any component
❌ New Tauri commands except getVisualizationData
❌ Imports from sp314-dsp, telemetry, rule-engine, M0, Aether
❌ Inline hex colors — CSS variables only
❌ Modifying xaak.rs, cpal, or any core crate
```

---

## 🏁 Exit Criteria

```bash
# Visual: cockpit matches reference mockups
# Functional: full flow works
# LOAD NEW → MASTER → FM5 → metrics visible → PLAY → EXPORT

bash infra/ci/checks/ui-isolation-check.sh
cargo test --workspace
just ci
git tag v0.14.0-ui
```

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 14
**Status:** 🔒 LOCKED


---