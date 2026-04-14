# lineos-rule-engine — Deterministic Rule Engine

**Document:** `lineos/m1/rule-engine/README.md`
**Version:** 1.1
**Date:** 2026-04-14
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6
**Based on:** coach-core README v1.3 (Anestis, 2026-03-29)
**Changes from v1.0:** 8 rules → 6 rules. `lufs_compliance` replaces
3 separate per-platform loudness rules. `Thresholds` now carries
`preset_name` + `target_lufs: Option<f32>` for the user-selected preset.
Rule-engine evaluates against selected preset only — Cockpit state machine
owns preset selection.

---

## 1. Purpose

`lineos-rule-engine` is the deterministic intelligence layer of LineOS.

It is a pure, deterministic rule engine that:
- Reads only `AnalysisReport` (from Golden Blob via telemetry + insights)
- Applies pure functions (rules)
- Produces `CoachFindings`
- Has zero LLM, zero randomness, zero side effects, zero raw audio access

It is the only authoritative layer that decides what is an "issue".

---

## 2. Core Principles (Unbreakable)

1. **Pure Functions Only** — every rule is pure: same input → same output
2. **No Side Effects** — zero I/O, zero global state, zero mutation
3. **No LLM in Core** — LLM is permitted only in Aether (Narrative Layer, Phase 5+)
4. **Determinism is Sacred** — any determinism violation is a blocking bug
5. **Extensibility without Drift** — new rules are added; existing rules never change behavior without a major version bump
6. **Thresholds from Schema** — no threshold is hardcoded; all read from `bmr-128.schema.json`

---

## 3. Architecture

```
AnalysisReport (input — canonical facts only)
    │
    ▼
for each Rule in RULES:            ← static array, compile-time known
    result = rule.check(&report, &thresholds)
    │
    ▼
CoachFindings (output)
    ├── issues: Vec<Issue>
    └── recommendation: String
```

`lineos-rule-engine` is pure logic. No UI, no LLM, no I/O, no mutable state.

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

Phase 4 scope: `QualityMetrics` + `ComplianceFlags` only.
`SpectralMetrics` and `DynamicsMetrics` are Phase 5+.

Contract schema: `lineos/shared/schema/analysis-report.schema.json`

### 4.2 Output: CoachFindings

```json
{
  "issues": [
    {
      "id": "lufs_too_high",
      "severity": "medium",
      "params": { "current": -12.0, "target": -14.0, "delta": 2.0 },
      "tags": ["platform:spotify", "platform:youtube"]
    }
  ],
  "recommendation": "Reduce gain to meet loudness target."
}
```

Severity values: `info | low | medium | high` — exactly these four.
Contract schema: `lineos/shared/schema/coach-findings.schema.json`

---

## 5. Rule Definition Model

```rust
pub struct Rule {
    pub id:    &'static str,
    pub check: fn(&AnalysisReport, &Thresholds) -> Option<Issue>,
    pub tags:  &'static [&'static str],
}
```

### 5.1 Thresholds

All thresholds are loaded from `bmr-128.schema.json` at startup
and passed as `&Thresholds` to every rule. No threshold is hardcoded.

```rust
pub struct Thresholds {
    pub spotify_lufs:         f32,  // from schema
    pub apple_lufs:           f32,  // from schema
    pub true_peak_max:        f32,  // from schema
    pub dynamic_range_min:    f32,  // from schema
    pub stereo_corr_min:      f32,  // from schema
    pub lufs_tolerance:       f32,  // ±0.5 LU
    // ... all BMR-128 thresholds
}
```

### 5.2 Example Rule

```rust
// rules/loudness.rs
pub fn lufs_compliance(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    let target = match t.target_lufs { Some(v) => v, None => return None };
    let delta = r.quality.lufs_integrated - target;
    if delta.abs() <= t.lufs_tolerance { return None; }
    let severity = if delta.abs() > 2.0 { Severity::High }
                   else if delta.abs() > 1.0 { Severity::Medium }
                   else { Severity::Low };
    Some(Issue {
        id: "lufs_compliance",
        severity,
        params: IssueParams { current: r.quality.lufs_integrated, target, delta },
        tags: vec![alloc::format!("platform:{}", t.preset_name)],
    })
}
```

---

## 6. Rule Registry

All rules registered in a static array (`rules/registry.rs`):

```rust
pub static RULES: &[Rule] = &[
    Rule { id: "lufs_compliance",          check: loudness::lufs_compliance,          tags: &["loudness"] },
    Rule { id: "true_peak_exceeded",       check: peak::true_peak_exceeded,           tags: &["peak", "critical"] },
    Rule { id: "dynamic_range_low",        check: dynamics::dynamic_range_low,        tags: &["dynamics"] },
    Rule { id: "lra_too_high",             check: dynamics::lra_too_high,             tags: &["dynamics"] },
    Rule { id: "stereo_correlation_weak",  check: stereo::stereo_correlation_weak,    tags: &["stereo"] },
    Rule { id: "dc_offset_detected",       check: stereo::dc_offset_detected,         tags: &["quality"] },
];
```

Execution is static and compile-time known.
Zero dynamic dispatch. Zero trait objects in the hot path.

**Rule ordering:** Rules are evaluated in declaration order.
No rule depends on the output of another rule. This is a hard invariant.

**Rule filtering:** The rule-engine always evaluates all rules.
Filtering specific rules (e.g. suppressing stereo warnings for mono content)
is the responsibility of the Cockpit UI — not the rule-engine.

---

## 7. Execution Pipeline

```
1. Load Thresholds from bmr-128.schema.json (once, at startup)
2. Build AnalysisReport from Golden Blob (via telemetry + insights)
3. For each Rule in RULES:
       result = rule.check(&report, &thresholds)
4. Collect all Some(issue) → Vec<Issue>
5. derive_recommendation(&issues) → String
6. Return CoachFindings
```

No branching, state, mutation, or caching that changes output.

**Recommendation derivation:** `derive_recommendation()` uses a static
priority mapping over issue IDs — `true_peak_exceeded` first, then by
severity. No LLM, no dynamic generation, no templates. Same issues →
same recommendation string. Always.

---

## 8. How to Add a New Rule

1. Create pure function in the appropriate module (`rules/loudness.rs`, `rules/peak.rs`, etc.)
2. Add entry to `RULES` static array in `rules/registry.rs`
3. Write unit test + determinism test + boundary value tests
4. **Never change the behavior of an existing rule without a major version bump**

```rust
// Step 1: pure function
pub fn broadcast_lufs_too_high(r: &AnalysisReport, t: &Thresholds) -> Option<Issue> {
    if r.quality.lufs_integrated > t.broadcast_lufs + t.lufs_tolerance {
        Some(Issue { id: "broadcast_lufs_too_high", severity: Severity::Medium, ... })
    } else { None }
}

// Step 2: register
Rule { id: "broadcast_lufs_too_high", check: loudness::broadcast_lufs_too_high, tags: &["loudness", "broadcast"] },

// Step 3: test
#[test]
fn test_broadcast_lufs_too_high() { ... }
```

---

## 9. Forbidden Patterns

```
❌ ML models or LLM calls
❌ Randomness / timestamps
❌ Global mutable state
❌ Async / I/O
❌ Fuzzy logic or heuristics
❌ Caching that changes output
❌ Dynamic dispatch in the hot path
❌ Hardcoded thresholds
❌ Rules that depend on other rules
❌ serde_json::Value at WASM boundary
❌ SpectralMetrics (Phase 5+)
❌ User context in rules (belongs to Aether Narrative Layer)
```

---

## 10. no_std Compatibility

`lineos-rule-engine` targets `no_std + alloc` for future WASM compatibility.

- Use `core::` and `alloc::` instead of `std::` where possible
- `serde_json::Value` in `IssueParams` is **native-only** — never crosses WASM boundary
- At WASM boundary use `serde_wasm_bindgen`

---

## 11. Testing Strategy

- **Determinism tests** — same input ×100 → identical output
- **Golden tests** — fixed snapshots (input → expected findings) with `insta`
- **Regression tests** — each rule has its own test suite
- **Boundary tests** — exactly at threshold, ±epsilon
- **Clean track test** — no issues for a compliant track

---

## 12. Performance

- O(N) where N = number of rules
- Each rule is O(1)
- Zero heap allocations in the hot path (where feasible)
- Zero I/O

---

## 13. Versioning

| Change type | Version bump |
|-------------|-------------|
| New rule | Minor |
| Change severity or threshold in existing rule | Major |
| Bugfix without output change | Patch |
| Change `AnalysisReport` struct | Major + schema version bump |

---

## 14. Future (Phase 5+)

- `SpectralMetrics` added to `AnalysisReport` → new spectral rules
- `DynamicsMetrics` → transient and punch rules
- Narrative Layer (Aether) wraps `CoachFindings` with LLM explanations
- `CoachFindings` remains the contract — Aether never modifies it

---

**Lead Architect:** Anestis
**System:** LineOS
**Document:** `lineos/m1/rule-engine/README.md`
**Version:** 1.0
**Date:** 2026-04-14
**Status:** 🔒 LOCKED
