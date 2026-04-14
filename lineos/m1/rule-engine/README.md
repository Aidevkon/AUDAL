# lineos-rule-engine — Deterministic Rule Engine

**Document:** `lineos/m1/rule-engine/README.md`
**Version:** 1.1
**Date:** 2026-04-14
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6

---

## 1. Purpose

`lineos-rule-engine` is the deterministic intelligence layer of LineOS.

It reads `AnalysisReport` (from Golden Blob via telemetry + insights),
applies pure functions (rules), and produces `CoachFindings`.

Zero LLM. Zero randomness. Zero side effects. Zero raw audio access.

It is the only authoritative layer that decides what is an "issue".

---

## 2. Core Principles (Unbreakable)

1. **Pure Functions Only** — every rule is pure: same input → same output
2. **No Side Effects** — zero I/O, zero global state, zero mutation
3. **No LLM in Core** — LLM is Aether (Narrative Layer, Phase 5+)
4. **Determinism is Sacred** — any violation is a blocking bug
5. **Extensibility without Drift** — new rules are added; existing rules never change behavior without a major version bump
6. **Thresholds from Schema** — all thresholds from `bmr-128.schema.json`; fallbacks in `unwrap_or()` only
7. **Preset-aware** — Cockpit selects preset; rule-engine evaluates against that preset only

---

## 3. Architecture

```
User selects preset (Cockpit)
    │
    ▼
Thresholds::from_schema_with_preset(&schema, preset)
    │
    ▼
AnalysisReport (from Golden Blob via telemetry + insights)
    │
    ▼
for each Rule in RULES:              ← static array, compile-time known
    result = rule.check(&report, &thresholds)
    │
    ▼
CoachFindings
    ├── issues: Vec<Issue>
    └── recommendation: String
```

No branching, state, mutation, or caching that changes output.

---

## 4. Input & Output

### 4.1 Input: AnalysisReport

```rust
pub struct AnalysisReport {
    pub quality:    QualityMetrics,    // LUFS, TP, LRA, correlation, dynamic range
    pub compliance: ComplianceFlags,   // per-platform pass/fail booleans
    pub version:    &'static str,
}
```

Phase 4 scope: `QualityMetrics` + `ComplianceFlags`.
`SpectralMetrics` and `DynamicsMetrics` are Phase 5+.

Contract schema: `lineos/shared/schema/analysis-report.schema.json`

### 4.2 Output: CoachFindings

```json
{
  "issues": [
    {
      "id": "lufs_compliance",
      "severity": "medium",
      "params": { "current": -12.0, "target": -14.0, "delta": 2.0 },
      "tags": ["platform:spotify"]
    }
  ],
  "recommendation": "Reduce gain to meet loudness target."
}
```

Severity: `info | low | medium | high` — exactly these four.
Contract schema: `lineos/shared/schema/coach-findings.schema.json`

---

## 5. Thresholds

All thresholds are loaded from `bmr-128.schema.json` for the
**user-selected preset**. Cockpit owns preset selection.

```rust
pub struct Thresholds {
    pub preset_name:         &'static str,  // e.g. "spotify"
    pub target_lufs:         Option<f32>,   // None = raw preset → lufs_compliance skips
    pub true_peak_max:       f32,           // from selected preset
    pub lufs_tolerance:      f32,           // ±0.5 LU
    pub dynamic_range_min:   f32,           // 6.0 dB
    pub stereo_corr_min:     f32,           // 0.8
    pub stereo_corr_warning: f32,           // 0.5
    pub dc_offset_max:       f32,           // 0.01
    pub lra_max:             f32,           // 14.0 LU
}
```

**Constructors:**

```rust
// For a specific preset (normal path — Cockpit provides preset)
Thresholds::from_schema_with_preset(&schema, "spotify")
Thresholds::from_schema_with_preset(&schema, "apple_music")
Thresholds::from_schema_with_preset(&schema, "broadcast")
Thresholds::from_schema_with_preset(&schema, "raw")  // target_lufs = None

// Shorthand — defaults to "spotify"
Thresholds::from_schema(&schema)
```

---

## 6. Rule Registry (6 Core Rules — v1.1)

```rust
pub static RULES: &[Rule] = &[
    Rule { id: "lufs_compliance",         check: loudness::lufs_compliance,        tags: &["loudness"] },
    Rule { id: "true_peak_exceeded",      check: peak::true_peak_exceeded,         tags: &["peak", "critical"] },
    Rule { id: "dynamic_range_low",       check: dynamics::dynamic_range_low,      tags: &["dynamics"] },
    Rule { id: "lra_too_high",            check: dynamics::lra_too_high,           tags: &["dynamics"] },
    Rule { id: "stereo_correlation_weak", check: stereo::stereo_correlation_weak,  tags: &["stereo"] },
    Rule { id: "dc_offset_detected",      check: stereo::dc_offset_detected,       tags: &["quality"] },
];
```

**Rule ordering:** Evaluated in declaration order.
No rule depends on the output of another. Hard invariant.

**Rule filtering:** Rule-engine always evaluates all rules.
Filtering (e.g. suppress stereo warnings for mono) is the Cockpit's responsibility.

---

## 7. Rule Specifications

### R001 — `lufs_compliance`
One LUFS value, one rule, one preset target.
```
target_lufs = None (raw preset) → None (skip)
delta = lufs_integrated - target_lufs
delta.abs() <= lufs_tolerance (0.5) → None
delta.abs() > 2.0  → High
delta.abs() > 1.0  → Medium
else               → Low
tag = "platform:{preset_name}"
```

### R002 — `true_peak_exceeded`
```
true_peak > true_peak_max → High (blocking)
```

### R003 — `dynamic_range_low`
```
dynamic_range < dynamic_range_min (6.0 dB) → Low
```

### R004 — `lra_too_high`
```
loudness_range > lra_max (14.0 LU) → Info
```

### R005 — `stereo_correlation_weak`
```
correlation < stereo_corr_warning (0.5) → High
correlation < stereo_corr_min (0.8)     → Medium
```

### R006 — `dc_offset_detected`
```
dc_offset.abs() > dc_offset_max (0.01) → Medium
```

---

## 8. Recommendation Derivation

`derive_recommendation()` uses static priority — no LLM, no dynamic generation.
Same issues → same recommendation. Always.

```
Priority order:
1. true_peak_exceeded         → "Apply brick-wall limiter..."
2. Any High severity          → "Critical issues detected..."
3. lufs_compliance (delta>0)  → "Reduce gain to meet loudness target."
4. lufs_compliance (delta<0)  → "Increase gain to meet loudness target."
5. Any Medium severity        → "Medium-priority issues detected..."
6. No issues                  → "Track is ready — proceed with export."
7. else                       → "Minor issues detected..."
```

---

## 9. How to Add a New Rule

1. Create pure function in appropriate module (`rules/loudness.rs`, etc.)
2. Add entry to `RULES` in `rules/registry.rs`
3. Write unit test + determinism test + boundary value tests
4. **Never change existing rule behavior without a major version bump**

```rust
// Step 1: pure function
pub fn broadcast_lufs_noncompliant(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    // ...
}

// Step 2: register (at end of RULES array)
Rule { id: "broadcast_lufs_noncompliant", check: loudness::broadcast_lufs_noncompliant, tags: &["loudness", "broadcast"] },

// Step 3: test
#[test]
fn test_broadcast_lufs_noncompliant() { ... }
```

---

## 10. Testing Strategy

- **Determinism tests** — same input ×100 → identical output
- **Golden tests** — snapshots with `insta`
- **Regression tests** — each rule has its own test suite
- **Boundary tests** — exactly at threshold, ±epsilon
- **Registry tests** — `RULES.len() == 6`, IDs unique, old IDs absent
- **Clean track test** — no issues for compliant track

---

## 11. Forbidden Patterns

```
❌ ML models or LLM calls
❌ Randomness / timestamps
❌ Global mutable state
❌ Async / I/O
❌ Fuzzy logic or heuristics
❌ Dynamic dispatch in the hot path
❌ Hardcoded thresholds (unwrap_or fallbacks are documented exceptions)
❌ Rules that depend on other rules
❌ serde_json::Value at WASM boundary
❌ SpectralMetrics / DynamicsMetrics (Phase 5+)
❌ Evaluating against all presets simultaneously (Cockpit selects one preset)
```

---

## 12. Performance

- O(N) where N = 6 rules
- Each rule O(1)
- Zero heap allocations in hot path
- Zero I/O

---

## 13. Versioning

| Change | Version bump |
|--------|-------------|
| New rule | Minor |
| Change severity or threshold behavior | Major |
| Bugfix without output change | Patch |
| Change `AnalysisReport` struct | Major + schema bump |

---

## 14. Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.1 | 2026-04-14 | `lufs_compliance` replaces `lufs_too_high` + `lufs_too_low` + `lufs_apple_too_high`. `Thresholds` carries `preset_name` + `target_lufs: Option<f32>` for selected preset. 8 rules → 6 rules. Cockpit owns preset selection. |
| 1.0 | 2026-04-14 | Initial — 8 rules, static RULES array, zero dynamic dispatch |

---

**Lead Architect:** Anestis
**System:** LineOS
**Document:** `lineos/m1/rule-engine/README.md`
**Version:** 1.1
**Status:** 🔒 LOCKED
