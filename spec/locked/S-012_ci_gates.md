# S-012 — Constitutional CI Gates

**Document:** `spec/locked/S-012_ci_gates.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Infra
**Depends on:** All specs (S-001 → S-011b)
**Used by:** CI/CD pipeline (canonical branch merge gate)
**Audit:** DeepSeek v0.1 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. N1: G-004 extended to check Apps→LineOS. N2: G-011 scope documented. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Constitutional CI Gates enforce constitutional rules at build time.
No code merges to canonical without passing all gates.

**One sentence:** A set of automated checks that prevent constitutional
violations from reaching the canonical branch.

---

## 2. Gate Registry

| ID | Gate | Script | Blocks merge |
|----|------|--------|--------------|
| G-001 | Aether Boundary | `aether-boundary-check.sh` | ✅ Yes |
| G-002 | ML Origin | `ml-origin-check.sh` | ✅ Yes |
| G-003 | LLM Contract | `llm-contract-check.sh` | ✅ Yes |
| G-004 | Layer Isolation | `layer-isolation-check.sh` | ✅ Yes |
| G-005 | No serde_json::Value | `no-value-check.sh` | ✅ Yes |
| G-006 | No whisper-rs | `no-whisper-check.sh` | ✅ Yes |
| G-007 | License Audit | `cargo deny check licenses` | ✅ Yes |
| G-008 | Determinism | `v3-determinism-check.sh` | ✅ Yes |
| G-009 | No rand in DSP | `no-rand-dsp-check.sh` | ✅ Yes |
| G-010 | No std::f32 in DSP | `no-std-float-check.sh` | ✅ Yes |
| G-011 | Spec Coverage | `spec-coverage-check.sh` | ⚠️ Warn |
| G-012 | Constitutional Bounds | `cargo test --workspace` | ✅ Yes |

---

## 3. Gate Specifications

### G-001 — Aether Boundary Check

```bash
#!/bin/bash
VIOLATIONS=0

# No direct HTTP clients in aether/adapters/ (must use adapter-runtime)
if grep -rn "reqwest\|ureq\|hyper" aether/adapters/ 2>/dev/null; then
    echo "❌ G-001: Direct HTTP client in aether/adapters/"
    VIOLATIONS=$((VIOLATIONS+1))
fi

# No raw serde_json::Value in adapter return types
if grep -rn "-> Result<serde_json::Value" aether/adapters/ 2>/dev/null; then
    echo "❌ G-001: Raw serde_json::Value in adapter return type"
    VIOLATIONS=$((VIOLATIONS+1))
fi

# validate_output() must be called in every adapter
for f in $(find aether/adapters/ -name "*.rs" 2>/dev/null); do
    if ! grep -q "validate_output" "$f"; then
        echo "❌ G-001: Missing validate_output() in $f"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
done

[ $VIOLATIONS -eq 0 ] && echo "✅ G-001: Aether boundary clean" || exit 1
```

---

### G-002 — ML Origin Check

```bash
#!/bin/bash
VIOLATIONS=0
ML_PATTERNS="ort::\|candle::\|tract::\|tch::\|tensorflow\|torch\|onnxruntime"

for dir in lineos/ pipelines/; do
    if grep -rn "$ML_PATTERNS" "$dir" 2>/dev/null | grep -v "//\|test"; then
        echo "❌ G-002: ML library found in $dir"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
done

[ $VIOLATIONS -eq 0 ] && echo "✅ G-002: ML origin clean" || exit 1
```

---

### G-003 — LLM Contract Check

```bash
#!/bin/bash
VIOLATIONS=0
FORBIDDEN="openai::\|anthropic::\|mistral_client\|llm_sdk"

if grep -rn "$FORBIDDEN" . --include="*.rs" \
    --exclude-dir="creator-os/shared/adapter-runtime" 2>/dev/null; then
    echo "❌ G-003: Direct LLM SDK call outside adapter-runtime"
    VIOLATIONS=$((VIOLATIONS+1))
fi

[ $VIOLATIONS -eq 0 ] && echo "✅ G-003: LLM contract clean" || exit 1
```

---

### G-004 — Layer Isolation Check

```bash
#!/bin/bash
VIOLATIONS=0

# Aether must not import lineos internals (lineos_types is allowed)
if grep -rn "use lineos_m0\|use sp314_dsp\|use sp314_nodes\|use xaak" \
    aether/ 2>/dev/null | grep -v "lineos_types\|//"; then
    echo "❌ G-004: Aether importing LineOS internals"
    VIOLATIONS=$((VIOLATIONS+1))
fi

# LineOS must not import aether
if grep -rn "use aether" lineos/ 2>/dev/null | grep -v "//"; then
    echo "❌ G-004: LineOS importing Aether"
    VIOLATIONS=$((VIOLATIONS+1))
fi

# Apps must not import LineOS internals directly
if grep -rn "use lineos_m0\|use sp314_dsp\|use sp314_nodes\|use xaak" \
    apps/ 2>/dev/null | grep -v "lineos_types\|//"; then
    echo "❌ G-004: Apps importing LineOS internals directly"
    VIOLATIONS=$((VIOLATIONS+1))
fi

[ $VIOLATIONS -eq 0 ] && echo "✅ G-004: Layer isolation clean" || exit 1
```

---

### G-005 — No serde_json::Value in Adapters

```bash
#!/bin/bash
if grep -rn "serde_json::Value" aether/adapters/ 2>/dev/null | grep -v "//"; then
    echo "❌ G-005: serde_json::Value in adapter I/O"
    exit 1
fi
echo "✅ G-005: No serde_json::Value in adapters"
```

---

### G-006 — No whisper-rs

```bash
#!/bin/bash
if grep -rn "whisper.rs\|whisper-rs\|whisper_rs" . \
    --include="*.toml" --include="*.rs" 2>/dev/null | grep -v "//"; then
    echo "❌ G-006: whisper-rs (C++ FFI) detected"
    exit 1
fi
echo "✅ G-006: No whisper-rs"
```

---

### G-007 — License Audit

```bash
cargo deny check licenses
```

`deny.toml`:
```toml
[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception"]
deny  = ["GPL-2.0", "GPL-3.0", "LGPL-2.0", "LGPL-3.0", "AGPL-3.0"]
```

---

### G-008 — Determinism Check

```bash
#!/bin/bash
cd lineos/m1/sp314-dsp
cargo test --test determinism -- --nocapture
if [ $? -ne 0 ]; then
    echo "❌ G-008: Golden hash mismatch — update tests/determinism.rs if intentional"
    exit 1
fi
echo "✅ G-008: Determinism check passed"
```

**Golden hash update protocol:**
1. Run `sp314_master test.wav` → get new hash
2. Update `tests/determinism.rs`
3. Commit: `chore: update golden hash — <reason>`

---

### G-009 — No rand in DSP

```bash
#!/bin/bash
VIOLATIONS=0

for dir in lineos/ pipelines/ aether/chaos/ aether/semantic/ aether/mapping/; do
    if grep -rn "rand::\|thread_rng\|random()" "$dir" 2>/dev/null \
        --include="*.rs" | grep -v "//\|#\[cfg(test\]"; then
        echo "❌ G-009: rand usage in deterministic path: $dir"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
done

[ $VIOLATIONS -eq 0 ] && echo "✅ G-009: No rand in DSP paths" || exit 1
```

---

### G-010 — No std::f32 in DSP

```bash
#!/bin/bash
VIOLATIONS=0
PATTERNS="\.sqrt()\|\.log()\|\.log2()\|\.log10()\|\.powf(\|\.sin()\|\.cos()\|\.tan()\|\.exp()"

for dir in lineos/m1/sp314-dsp/src/ pipelines/; do
    if grep -rn "$PATTERNS" "$dir" 2>/dev/null \
        --include="*.rs" | grep -v "//\|libm\|#\[cfg(test\]"; then
        echo "❌ G-010: std::f32 method in DSP path: $dir"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
done

[ $VIOLATIONS -eq 0 ] && echo "✅ G-010: libm only in DSP" || exit 1
```

---

### G-011 — Spec Coverage (Warning only)

**Scope:** `aether/` modules only (v1.0).
`lineos/` and `pipelines/` coverage can be added in v1.1.

```bash
#!/bin/bash
WARNINGS=0

for module in chaos semantic mapping intent personas tuning; do
    if ! ls spec/locked/S-0*"$module"* 2>/dev/null | grep -q .; then
        echo "⚠️  G-011: No locked spec for aether/$module/"
        WARNINGS=$((WARNINGS+1))
    fi
done

[ $WARNINGS -gt 0 ] && echo "⚠️  G-011: $WARNINGS warning(s) — non-blocking"
echo "✅ G-011: Spec coverage check complete"
exit 0  # Never blocks merge
```

---

### G-012 — Constitutional Bounds (cargo test)

```bash
cargo test --workspace
if [ $? -ne 0 ]; then
    echo "❌ G-012: Contract tests FAILED"
    exit 1
fi
echo "✅ G-012: All contract tests passed"
```

---

## 4. CI Pipeline Integration

```yaml
name: Constitutional Gates
on:
  pull_request:
    branches: [canonical]

jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: G-001 Aether Boundary
        run: bash infra/ci/checks/aether-boundary-check.sh
      - name: G-002 ML Origin
        run: bash infra/ci/checks/ml-origin-check.sh
      - name: G-003 LLM Contract
        run: bash infra/ci/checks/llm-contract-check.sh
      - name: G-004 Layer Isolation
        run: bash infra/ci/checks/layer-isolation-check.sh
      - name: G-005 No serde_json::Value
        run: bash infra/ci/checks/no-value-check.sh
      - name: G-006 No whisper-rs
        run: bash infra/ci/checks/no-whisper-check.sh
      - name: G-007 License Audit
        run: cargo deny check licenses
      - name: G-008 Determinism
        run: bash infra/ci/checks/v3-determinism-check.sh
      - name: G-009 No rand in DSP
        run: bash infra/ci/checks/no-rand-dsp-check.sh
      - name: G-010 No std::f32 in DSP
        run: bash infra/ci/checks/no-std-float-check.sh
      - name: G-011 Spec Coverage (warn)
        run: bash infra/ci/checks/spec-coverage-check.sh
        continue-on-error: true
      - name: G-012 Contract Tests
        run: cargo test --workspace
```

---

## 5. Gate Failure Protocol

| Gate | Fatal? | Action |
|------|--------|--------|
| G-001 to G-010 | ✅ Yes | Merge blocked. Fix before resubmitting. |
| G-011 | ❌ No | Warning only. Merge allowed. |
| G-012 | ✅ Yes | Merge blocked. Fix failing tests. |

---

## 6. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Gates deterministic | ✅ grep/bash — same code → same result |
| No false positives from comments | ✅ grep -v "//" |
| Golden hash reproducible | ✅ SHA-256 |

---

## 7. Performance Targets

| Gate | Expected runtime |
|------|-----------------|
| G-001 to G-010 (grep) | < 10s |
| G-007 (cargo deny) | < 30s |
| G-008 (determinism) | < 60s |
| G-012 (cargo test) | < 120s |
| **Total** | **< 5 minutes** |

---

## 8. Implementation Path

```
infra/ci/checks/
├── aether-boundary-check.sh   ← G-001
├── ml-origin-check.sh         ← G-002
├── llm-contract-check.sh      ← G-003
├── layer-isolation-check.sh   ← G-004 (updated: +Apps check)
├── no-value-check.sh          ← G-005
├── no-whisper-check.sh        ← G-006
├── no-rand-dsp-check.sh       ← G-009
├── no-std-float-check.sh      ← G-010
├── spec-coverage-check.sh     ← G-011
└── v3-determinism-check.sh    ← G-008 (existing)

deny.toml                      ← G-007
```

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-012_ci_gates.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*No violation reaches canonical.*
*Every gate is a constitutional promise.*
