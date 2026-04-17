# LineOS — Phase 13 Master Prompt

**Document:** `lineos/plan/phase-13/master-prompt.md`
**Version:** 1.0
**Phase:** 13 — MP3 Export + BMR-128 PDF Report
**Tag:** `v0.13.0-mp3`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6
**Date:** 2026-04-17

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.12b.0-transport
pwd && git branch --show-current
just ci

# Verify libmp3lame is installed
pkg-config --libs mp3lame && echo "✅ libmp3lame OK" || echo "❌ run: sudo apt install libmp3lame-dev"
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat docs/licenses/LAME-LGPL-NOTICE.md   # LGPL dynamic linking requirement
cat lineos/plan/phase-13/task-decomposition.md
```

---

## 🎯 Phase 13 Goal

**Two deliverables:**

### 13A — MP3 Export
Add MP3 export via LAME (dynamic linking, LGPL compliant).
Format joins WAV + FLAC + Opus in the export provider set.

### 13B — BMR-128 PDF Report
Generate a human-readable compliance report PDF from the
Golden Blob metadata. Exported alongside the audio file.

**At end of phase:**
- MP3 button in export selector → writes mastered.mp3
- PDF report button → writes mastered-bmr128.pdf
- LAME linked dynamically (LGPL compliance)
- `docs/licenses/LAME-LGPL-NOTICE.md` committed
- `just ci` passes
- Tag `v0.13.0-mp3`

---

## 🔒 Forbidden in Phase 13

```
❌ Static linking of LAME (LGPL violation)
❌ FFmpeg or subprocess encoding
❌ Re-running DSP pipeline during export
❌ Re-measuring loudness during export
❌ PCM in IPC or SessionStateJson
❌ Modifying xaak.rs or cpal
```

---

## 🏁 Exit Criteria

```bash
# MP3 export
ls ~/Music/StillAir/exports/*.mp3

# PDF report
ls ~/Music/StillAir/exports/*.pdf

# LAME dynamic link verified
ldd target/debug/m0d | grep mp3lame

# LGPL notice committed
cat docs/licenses/LAME-LGPL-NOTICE.md

cargo test --workspace
just ci
```

Commit + tag `v0.13.0-mp3`.

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 13
**Status:** 🔒 LOCKED


---