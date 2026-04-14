# LineOS — Phase 4 Master Prompt

**Document:** `lineos/plan/phase-4/master-prompt.md`
**Version:** 1.0
**Phase:** 4 — rule-engine (coach-core)
**Tag:** `v0.4.0-rule-engine`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6
**Date:** 2026-04-14

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.3.0-telemetry
pwd && git branch --show-current
git status --short
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat lineos/constitution/lineos-constitution.md      # §07 rule-engine
cat lineos/architecture/lineos-architecture.md      # §2.4 rule-engine row
cat lineos/plan/phase-4/task-decomposition.md
```

**Key rules for this phase:**
- rule-engine is **deterministic** — no LLM, no Aether, no randomness
- rule-engine **evaluates** — never modifies audio, never re-measures
- All rules are pure functions: same input → same output, always
- All thresholds from `bmr-128.schema.json` — never hardcoded
- Static `RULES` array — zero dynamic dispatch
- `CoachFindings` is the output contract — not natural language
- Severity values: `info | low | medium | high` (exactly these four)
- `SpectralMetrics` is Phase 5+ — Phase 4 uses QualityMetrics + ComplianceFlags only

---

## 🎯 Phase 4 Goal

Build `lineos-rule-engine` — the deterministic coach-core that transforms
`AnalysisReport` into `CoachFindings`.

```
AnalysisReport
  ├── quality_metrics   (from Golden Blob via telemetry)
  └── compliance_flags  (from insights)
       │
       ▼
  rule-engine (pure deterministic rules)
       │
       ▼
  CoachFindings
  ├── issues: Vec<Issue>   (id, severity, params, tags)
  └── recommendation: String
```

At end of phase:
- `lineos/m1/rule-engine/` crate compiles
- Minimum 8 rules covering loudness, true peak, dynamics, stereo
- `CoachFindings` is JSON-serializable
- Static `RULES` array — zero dynamic dispatch
- All thresholds from `bmr-128.schema.json`
- Determinism test: same input ×100 → identical output
- `just ci` passes
- Tag `v0.4.0-rule-engine`

---

## 🔒 Forbidden in Phase 4

```
❌ LLM calls — rule-engine is deterministic only
❌ Aether imports
❌ Hardcoded thresholds
❌ Dynamic dispatch in the rule hot path
❌ Rules that depend on other rules
❌ User context in rules (belongs to Narrative Layer — Aether)
❌ serde_json::Value crossing WASM boundary
❌ SpectralMetrics (Phase 5+)
❌ Modifying sp314-dsp, telemetry, metadata, or insights
```

---

## 🏁 Exit Criteria

```bash
cargo test -p lineos-rule-engine       # all pass
just check-thresholds                  # ✅ no hardcoded values
just check-ml-origin                   # ✅ no ML in lineos/
just ci                                # ✅
just deny                              # ✅
```

Commit + tag `v0.4.0-rule-engine`.

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 4 — rule-engine
**Status:** 🔒 LOCKED


---

