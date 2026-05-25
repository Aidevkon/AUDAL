#!/bin/bash
# sp314-dsp v3.0.0 Determinism Gate
# Enforces: same input → bit-identical output
# Runs test_engine binary and checks the golden hash.

set -e

echo "Running determinism check..."

# Run test_engine and capture hash
set +e
OUTPUT=$(cargo run --features cli --bin test_engine 2>&1)
set -e
echo "$OUTPUT"

# Extract the hash from the output
ACTUAL_HASH=$(echo "$OUTPUT" | grep -oE '[a-f0-9]{64}' | tail -1)

if [ -z "$ACTUAL_HASH" ]; then
  echo "❌ DETERMINISM GATE FAILED: could not extract hash from test_engine output"
  exit 1
fi

# Read expected hash from determinism.rs
EXPECTED_HASH=$(grep -oE '[a-f0-9]{64}' tests/determinism.rs | head -1)

if [ -z "$EXPECTED_HASH" ]; then
  echo "❌ DETERMINISM GATE FAILED: could not read expected hash from tests/determinism.rs"
  exit 1
fi

echo ""
echo "Expected hash: $EXPECTED_HASH"
echo "Actual hash:   $ACTUAL_HASH"

if [ "$ACTUAL_HASH" = "$EXPECTED_HASH" ]; then
  echo ""
  echo "✅ Determinism gate PASSED — hashes match."
  exit 0
else
  echo ""
  echo "❌ DETERMINISM GATE FAILED — hash mismatch."
  echo "   The DSP output has changed. If intentional, update tests/determinism.rs."
  exit 1
fi

echo "Running E11 assembly check..."
bash infra/ci/checks/e11-assembly-check.sh
