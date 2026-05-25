#!/bin/bash
# sp314-dsp v3.0.0 License Gate
# Enforces: MIT/Apache-2.0 only — no GPL/LGPL
# Uses cargo deny (must be installed).

set -e

echo "Running license check..."

# Check if cargo-deny is installed
if ! command -v cargo-deny &> /dev/null && ! cargo deny --version &> /dev/null 2>&1; then
  echo "Installing cargo-deny..."
  cargo install cargo-deny --quiet
fi

# Run cargo deny
set +e
OUTPUT=$(cargo deny check licenses 2>&1)
EXIT_CODE=$?
set -e

echo "$OUTPUT"

if [ $EXIT_CODE -eq 0 ]; then
  echo ""
  echo "✅ License gate PASSED — MIT/Apache-2.0 only."
  exit 0
else
  echo ""
  echo "❌ LICENSE GATE FAILED — non-compliant license detected."
  echo "   Check deny.toml and remove GPL/LGPL dependencies."
  exit 1
fi
