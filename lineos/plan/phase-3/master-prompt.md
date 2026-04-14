# LineOS — Phase 3 Master Prompt

**Document:** `lineos/plan/phase-3/master-prompt.md`
**Version:** 1.0
**Phase:** 3 — Telemetry + Metadata + Insights
**Tag:** `v0.3.0-telemetry`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6
**Date:** 2026-04-14

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.2.0-dsp
pwd && git branch --show-current
git status --short
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat lineos/constitution/lineos-constitution.md      # §07 M1 services
cat lineos/architecture/lineos-architecture.md      # §2.4 M1 services table
cat lineos/plan/phase-3/task-decomposition.md
```

**Key rules:**
- Telemetry, metadata, insights are **comparators** — they read the Golden Blob, never re-measure raw audio
- All thresholds from `bmr-128.schema.json` — never hardcoded
- No DSP code in these services — DSP lives in sp314-dsp only
- BS.1770-4 canonical values already computed in Golden Blob by sp314-dsp

---

## 🎯 Phase 3 Goal

Build the three M1 measurement services that sit downstream of sp314-dsp:

```
Golden Blob (from sp314-dsp)
    │
    ├── telemetry/    → EBU R128 full report (adds LRA, momentary, short-term)
    ├── metadata/     → BMR-128 report + project manifest
    └── insights/     → compliance comparator (pass/fail per preset)
```

At end of phase:
- `lineos/m1/telemetry/` crate: computes LRA + momentary + short-term from Golden Blob samples
- `lineos/m1/metadata/` crate: generates structured JSON reports
- `lineos/m1/insights/` crate: evaluates pass/fail against all presets
- All three compile native + WASM
- All tests pass including determinism
- `just ci` passes
- Tag `v0.3.0-telemetry`

---

## 🔒 Forbidden in Phase 3

```
❌ Re-measuring raw audio in telemetry/metadata/insights
❌ Hardcoded LUFS/TP thresholds
❌ DSP code outside sp314-dsp
❌ std::f32 methods — use libm
❌ Modifying sp314-dsp or M0 code
```

---

## 🏁 Exit Criteria

```bash
cargo test --workspace          # all pass
just check-thresholds           # ✅
just check-float-methods        # ✅
just validate-schemas           # ✅
just ci                         # ✅
just deny                       # licenses ok
```

Commit + tag `v0.3.0-telemetry`.

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 3
**Status:** 🔒 LOCKED
