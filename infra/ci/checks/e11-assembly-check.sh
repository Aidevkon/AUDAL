#!/bin/bash
# E11 Assembly Gate
# Verifies e11.wasm exists, manifest is present, and SHA256 matches.

set -e
ERRORS=0

WASM_PATH="lineos/m0/assets/wasm/e11.wasm"
MANIFEST_PATH="lineos/m0/assets/manifests/e11-manifest.json"

# Check wasm exists
if [ ! -f "$WASM_PATH" ]; then
  echo "❌ E11 GATE FAILED: $WASM_PATH not found"
  ERRORS=$((ERRORS + 1))
else
  echo "✅ e11.wasm exists"
fi

# Check manifest exists
if [ ! -f "$MANIFEST_PATH" ]; then
  echo "❌ E11 GATE FAILED: $MANIFEST_PATH not found"
  ERRORS=$((ERRORS + 1))
else
  echo "✅ e11-manifest.json exists"
fi

# Verify SHA256 matches manifest
if [ -f "$WASM_PATH" ] && [ -f "$MANIFEST_PATH" ]; then
  ACTUAL_SHA=$(sha256sum "$WASM_PATH" | cut -d' ' -f1)
  EXPECTED_SHA=$(grep '"sha256"' "$MANIFEST_PATH" | grep -oE '[a-f0-9]{64}')

  if [ "$ACTUAL_SHA" = "$EXPECTED_SHA" ]; then
    echo "✅ SHA256 match: $ACTUAL_SHA"
  else
    echo "❌ E11 GATE FAILED: SHA256 mismatch"
    echo "   Expected: $EXPECTED_SHA"
    echo "   Actual:   $ACTUAL_SHA"
    ERRORS=$((ERRORS + 1))
  fi
fi

# Check size is reasonable (between 100KB and 2MB)
if [ -f "$WASM_PATH" ]; then
  SIZE=$(wc -c < "$WASM_PATH")
  if [ "$SIZE" -lt 100000 ] || [ "$SIZE" -gt 2000000 ]; then
    echo "❌ E11 GATE FAILED: unexpected size ${SIZE} bytes"
    ERRORS=$((ERRORS + 1))
  else
    echo "✅ Size: ${SIZE} bytes (within limits)"
  fi
fi

if [ $ERRORS -gt 0 ]; then
  echo ""
  echo "❌ E11 assembly gate FAILED with $ERRORS error(s)."
  exit 1
fi

echo ""
echo "✅ E11 assembly gate PASSED."
exit 0
