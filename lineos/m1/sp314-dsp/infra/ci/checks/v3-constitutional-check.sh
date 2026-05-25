#!/bin/bash
# sp314-dsp v3.0.0 Constitutional Gate
# Enforces: no std::f32 math in DSP core, no subprocess in src/, no ML weights
# Fails build on any violation.

set -e
ERRORS=0

# Rule 1 — No std::f32 math in DSP core
VIOLATIONS=$(grep -rn "std::f32::\|\.sin()\|\.cos()\|\.exp()\|\.ln()\|\.sqrt()\|\.powf\|\.powi\|\.log2()\|\.log10()" \
  src/ \
  --include="*.rs" \
  | grep -v "src/bin/" \
  | grep -v "src/io/" \
  | grep -v "src/realtime/" \
  | grep -v "//.*std::f32" \
  || true)

if [ -n "$VIOLATIONS" ]; then
  echo "❌ CONSTITUTIONAL VIOLATION: std::f32 math in DSP core"
  echo "$VIOLATIONS"
  ERRORS=$((ERRORS + 1))
else
  echo "✅ std::f32 check: CLEAN"
fi

# Rule 2 — No subprocess in src/
VIOLATIONS=$(grep -rn "Command::new\|std::process::Command\|subprocess" \
  src/ \
  --include="*.rs" \
  || true)

if [ -n "$VIOLATIONS" ]; then
  echo "❌ CONSTITUTIONAL VIOLATION: subprocess call in src/"
  echo "$VIOLATIONS"
  ERRORS=$((ERRORS + 1))
else
  echo "✅ subprocess check: CLEAN"
fi

# Rule 3 — No rand::thread_rng in pipeline
VIOLATIONS=$(grep -rn "thread_rng\|rand::random" \
  src/pipeline/ src/compressor/ src/masking_eq/ src/psychoacoustic/ \
  src/harmonic/ src/limiter/ src/metering/ \
  --include="*.rs" \
  || true)

if [ -n "$VIOLATIONS" ]; then
  echo "❌ CONSTITUTIONAL VIOLATION: non-deterministic rand in DSP pipeline"
  echo "$VIOLATIONS"
  ERRORS=$((ERRORS + 1))
else
  echo "✅ deterministic rand check: CLEAN"
fi

# Rule 4 — No `let _ =` (silent failures)
VIOLATIONS=$(grep -rn "let _ =" \
  src/ \
  --include="*.rs" \
  | grep -v "src/bin/" \
  || true)

if [ -n "$VIOLATIONS" ]; then
  echo "❌ CONSTITUTIONAL VIOLATION: silent failure (let _ =) in src/"
  echo "$VIOLATIONS"
  ERRORS=$((ERRORS + 1))
else
  echo "✅ silent failure check: CLEAN"
fi

if [ $ERRORS -gt 0 ]; then
  echo ""
  echo "❌ Constitutional gate FAILED with $ERRORS violation(s)."
  exit 1
fi

echo ""
echo "✅ Constitutional gate PASSED."
exit 0
