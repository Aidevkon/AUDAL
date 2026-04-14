# LineOS — Phase 2 Master Prompt

**Document:** `lineos/plan/phase-2/master-prompt.md`
**Version:** 1.0
**Phase:** 2 — sp314-dsp (Audio Mastering Engine)
**Tag:** `v0.2.0-dsp`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6
**Date:** 2026-04-14

---

## ⚠️ PRE-CONDITIONS

```bash
# 1. Phase 1 tag exists
git tag | grep v0.1.0-m0

# 2. Working directory and branch
pwd && git branch --show-current
# Expected: .../creator-os, canonical

# 3. Clean working tree
git status --short
# Expected: empty

# 4. Source material present
ls ~/Documents/sonido.io/soundmaster/crates/sm-core/src/pipeline/
# Expected: stage1_analyze.rs through stage8_dither.rs
```

**Any failure → STOP. Report. Do not proceed.**

---

## 📖 Reading Order (Mandatory)

```bash
cat creator-os/constitution/creator-os-constitution.md   # §05 ML Origin Rule
cat lineos/constitution/lineos-constitution.md           # §05 sp314-dsp rules
cat lineos/architecture/lineos-architecture.md           # §2.2 pipeline stages
cat lineos/plan/phase-2/task-decomposition.md
```

**Key rules for this phase:**
- `libm` only in DSP — never `std::f32` methods
- No `rand_core` dependency — `XorShiftRng` is inline
- No hardcoded thresholds — all from `bmr-128.schema.json`
- `no_std + alloc` target
- Deterministic: same input + same seed → identical binary output

---

## 🎯 Phase 2 Goal

Extract, adapt, and verify `sm-core` DSP pipeline as `sp314-dsp` — the
single source of DSP truth in LineOS.

At the end of this phase:
- `sp314-dsp` crate compiles as `no_std + alloc`
- All 8 pipeline stages ported from `sm-core`
- `fast_tanh` (Pade) replaces any `std::f32::tanh()` usage
- `XorShiftRng` implemented inline — no `rand_core` dependency
- All thresholds loaded from `lineos/shared/schema/bmr-128.schema.json`
- WASM build: `cargo build --target wasm32-unknown-unknown -p sp314-dsp`
- Native build: `cargo build --release -p sp314-dsp`
- Determinism test: pipeline run ×2 with same input + seed → binary diff = 0
- `just ci` passes

---

## 🔒 Forbidden in Phase 2

```
❌ std::f32::tanh() or any std float methods in DSP code
❌ rand_core or any external RNG dependency in sp314-dsp
❌ Hardcoded LUFS/TP thresholds — must load from bmr-128.schema.json
❌ Copying engine-wasm adapter code (ports/, storage/, sync/)
❌ Any network or filesystem calls in sp314-dsp
❌ serde_json in sp314-dsp (no_std — use serde with no_std features)
❌ Modifying M0 code
❌ Modifying Phase 1 files
```

---

## 📦 Source Material

```
~/Documents/sonido.io/soundmaster/crates/sm-core/src/
├── pipeline/
│   ├── constants.rs     ← thresholds (will be replaced by schema)
│   ├── mod.rs           ← MasteringPipeline + XorShiftRng
│   ├── stage1_analyze.rs
│   ├── stage2_eq.rs
│   ├── stage3_deess.rs
│   ├── stage4_compress.rs
│   ├── stage5_saturate.rs  ← fast_tanh (Pade) ✅ no std::f32
│   ├── stage6_stereo.rs
│   ├── stage7_limit.rs
│   └── stage8_dither.rs
├── dsp/
│   └── biquad.rs        ← libm ✅ already correct
└── analysis.rs          ← AnalysisAccumulator
```

**Do NOT copy:**
- `ports/` — engine-wasm specific
- `orchestrator.rs` — replaced by pipeline DAG
- `types/session.rs` — replaced by Creator OS contracts
- `sync.rs` — cloud sync, not DSP

---

## 🏁 Exit Criteria (Gate)

```bash
# 1. Compiles native
cargo build --release -p sp314-dsp
# Expected: no errors

# 2. Compiles WASM
cargo build --target wasm32-unknown-unknown -p sp314-dsp
# Expected: no errors

# 3. All tests pass
cargo test -p sp314-dsp
# Expected: all pass including determinism test

# 4. Determinism test (explicit)
just test-determinism
# Expected: ✅ binary diff = 0

# 5. No std float methods in DSP
just check-float-methods
# Expected: ✅

# 6. No hardcoded thresholds
just check-thresholds
# Expected: ✅

# 7. Full CI
just ci
# Expected: ✅

# 8. cargo deny
just deny
# Expected: advisories ok, bans ok, licenses ok
```

**All pass → commit + tag:**

```bash
git add -A
git commit -m "feat(dsp): Phase 2 — sp314-dsp mastering engine

- 8-stage pipeline ported from sm-core
- no_std + alloc, wasm32-unknown-unknown ✅
- libm-only float math ✅
- XorShiftRng inline — no rand_core dependency ✅
- Thresholds from bmr-128.schema.json ✅
- Determinism test: binary diff = 0 ✅
- WASM artifact → lineos/m0/assets/wasm/sp314-dsp.wasm

Authority: LineOS Constitution v2.0 · Creator OS Constitution v2.6"

git tag v0.2.0-dsp
```

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 2 — sp314-dsp
**Status:** 🔒 LOCKED
