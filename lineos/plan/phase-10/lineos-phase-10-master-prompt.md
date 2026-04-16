# LineOS — Phase 10 Master Prompt

**Document:** `lineos/plan/phase-10/master-prompt.md`
**Version:** 1.0
**Phase:** 10 — Export (WAV + FLAC + Opus)
**Tag:** `v0.10.0-export`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6 · Amendment A-002
**Date:** 2026-04-16

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.9.0-telemetry
pwd && git branch --show-current
git status --short
cargo check -p m0d
cargo check -p stillair
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat creator-os/constitution/amendments/A-002-ui-disposable.md  # §2, §3
cat lineos/m0/m0-daemon/src/handlers/master.rs   # blob_store, run_dsp
cat lineos/m0/m0-daemon/src/handlers/export.rs   # existing export stub
cat lineos/plan/phase-10/task-decomposition.md
```

---

## 🎯 Phase 10 Goal

Implement real audio export from the Golden Blob.

**Formats (Phase 10):**
- **WAV** — lossless, 32-bit float, 48000 Hz — for DAWs
- **FLAC** — lossless, direct from Golden Blob bytes — near-free
- **Opus** — lossy, high-quality preview/sharing — pure Rust

**Where:** Local filesystem only. User selects output directory
via Tauri file dialog. Default: `~/Music/StillAir/exports/`

**What gets exported:**
- Audio file (WAV / FLAC / Opus)
- Sidecar JSON: `<filename>.stillair.json` — Golden Blob metadata
  (loudness, quality, compliance, provenance — no audio bytes)

**At end of phase:**
- EXPORT button in FM5 opens native save dialog
- User selects format + location
- File written to disk
- Sidecar JSON written alongside
- FM5 shows "Export complete" confirmation
- `just ci` passes
- Tag `v0.10.0-export`

---

## 🔒 Forbidden in Phase 10

```
❌ FFmpeg or any subprocess for encoding
❌ MP3 export (Phase 11+)
❌ Cloud upload (Phase 11+)
❌ Re-running the DSP pipeline during export
❌ Re-measuring loudness during export
❌ Modifying the Golden Blob after creation
❌ Export button visible/enabled before FM5
❌ std::f32 in audio math — use libm
```

---

## Library Decisions (Binding)

| Format | Crate | License | Where |
|--------|-------|---------|-------|
| WAV write | `hound` | MIT | m0d |
| FLAC | direct bytes from GoldenBlob | — | m0d |
| Opus | `opus` crate or `audiopus` | BSD | m0d |

**Note on FLAC:** The Golden Blob already stores `flac_bytes: Vec<u8>`.
FLAC export = write those bytes directly to disk. Zero re-encoding.

**Note on Opus:** Use `audiopus` (pure Rust bindings to libopus).
If `audiopus` has C dependency issues, fall back to `opus-rs`.

---

## 🏁 Exit Criteria

```bash
# Manual test
# FM5 → click EXPORT → select WAV → verify file exists
ls ~/Music/StillAir/exports/*.wav

# Sidecar JSON exists
ls ~/Music/StillAir/exports/*.stillair.json

# FLAC export
ls ~/Music/StillAir/exports/*.flac

cargo test --workspace
just ci
```

Commit + tag `v0.10.0-export`.

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 10
**Status:** 🔒 LOCKED


---