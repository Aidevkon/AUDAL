# LineOS — Phase 9 Master Prompt

**Document:** `lineos/plan/phase-9/master-prompt.md`
**Version:** 1.0
**Phase:** 9 — Telemetry Fix + Coach Prompt Schema
**Tag:** `v0.9.0-telemetry`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6
**Date:** 2026-04-16

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.8.0-coach
pwd && git branch --show-current
git status --short

# Verify current LRA is 0.0 (the bug we're fixing)
curl -s -X POST http://127.0.0.1:7400/master \
  -H "Content-Type: application/json" \
  -d '{"audio_path":"/home/aidevcon/Music/gargar.mp3","preset_id":"spotify"}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print('blob_id:', d['blob_id'])"
# Then fetch blob and verify lra: 0.0
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat lineos/m1/telemetry/src/lib.rs          # existing telemetry API
cat lineos/m0/m0-daemon/src/handlers/master.rs  # where telemetry must be wired
cat lineos/plan/phase-9/task-decomposition.md
```

---

## 🎯 Phase 9 Goal

**Two deliverables:**

### 9A — Telemetry Fix
Wire `lineos-telemetry` into the M0 mastering pipeline so that
LRA, momentary LUFS, and short-term LUFS are computed from real
BS.1770-4 windowed analysis.

After this phase:
- `lra` ≠ 0.0 for real audio (should be 4–14 LU for typical music)
- `momentary_lufs` ≠ `integrated_lufs` (different time windows)
- `short_term_lufs` ≠ `integrated_lufs` (3s window)

### 9B — Coach Prompt Schema
Move hardcoded prompt string from `coach_adapter.rs` to
`apps/stillair/src-tauri/assets/coach_prompt.toml`.

Loaded at runtime — tweak without rebuild.

---

## 🔒 Forbidden in Phase 9

```
❌ Modifying sp314-dsp pipeline stages
❌ Modifying the rule-engine rules
❌ Modifying frontend Cockpit layout
❌ Adding new telemetry metrics (fix existing ones only)
❌ Hardcoded LUFS windows (must read from bmr-128.schema.json)
❌ std::f32 methods in telemetry math — use libm
```

---

## 🏁 Exit Criteria

```bash
# LRA is real (not 0.0)
curl -s http://127.0.0.1:7400/blob/<id> | python3 -m json.tool | grep lra
# Expected: "lra": 4.0-14.0 (not 0.0)

# Momentary ≠ Integrated
# Short-term ≠ Integrated

# Coach prompt loads from toml
ls apps/stillair/src-tauri/assets/coach_prompt.toml

cargo test --workspace
just ci
```

Commit + tag `v0.9.0-telemetry`.

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 9
**Status:** 🔒 LOCKED


---