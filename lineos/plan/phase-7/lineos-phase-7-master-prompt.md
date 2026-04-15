# LineOS — Phase 7 Master Prompt

**Document:** `lineos/plan/phase-7/master-prompt.md`
**Version:** 1.0
**Phase:** 7 — Audio Decode (symphonia)
**Tag:** `v0.7.0-decode`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6
**Date:** 2026-04-15

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.6.0-wired
pwd && git branch --show-current
git status --short

# Verify M0 mastering endpoint works (from Phase 6)
curl -s http://127.0.0.1:7400/health | python3 -m json.tool
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat lineos/constitution/lineos-constitution.md     # §05 sp314-dsp rules
cat lineos/m0/constitution/m0-constitution.md      # §03 M0 responsibilities
cat lineos/plan/phase-7/task-decomposition.md
```

**Key architectural decision (locked):**
- Audio decode happens in M0 (`handlers/master.rs`) — NOT in sp314-dsp
- sp314-dsp receives ONLY f32 PCM — never compressed audio
- symphonia lives in m0d Cargo.toml — not in sp314-dsp
- This is the single source of DSP truth invariant

---

## 🎯 Phase 7 Goal

Replace `bytes_to_f32_samples()` stub in `handlers/master.rs` with
real symphonia decode. After this phase, dropping a WAV/FLAC/MP3/AIFF
file produces a Golden Blob with accurate loudness metrics.

**At end of phase:**
- symphonia decodes WAV, FLAC, MP3, AIFF to f32 PCM
- Real sample_rate, channels, duration extracted from file
- sp314-dsp receives proper f32 PCM
- Golden Blob loudness metrics are accurate
- `cargo test -p m0d` passes including decode tests
- `just ci` passes
- Tag `v0.7.0-decode`

---

## 🔒 Forbidden in Phase 7

```
❌ symphonia in sp314-dsp — decode is M0 responsibility
❌ std::f32 methods in DSP — use libm
❌ Network calls during decode
❌ Modifying sp314-dsp pipeline stages
❌ Modifying frontend or Cockpit code
```

---

## 🏁 Exit Criteria

```bash
# Real decode test
curl -X POST http://127.0.0.1:7400/master \
  -H "Content-Type: application/json" \
  -d '{"audio_path": "/home/aidevcon/Downloads/gargar.mp3", "preset_id": "spotify"}'
# Expected: blob_id + integrated_lufs in range [-40, 0]

cargo test -p m0d
just ci
just deny
```

Commit + tag `v0.7.0-decode`.

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 7
**Status:** 🔒 LOCKED


---