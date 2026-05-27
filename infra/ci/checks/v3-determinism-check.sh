#!/bin/bash
# G-008: Determinism Check — golden hash
# Wraps existing determinism.rs test in sp314-dsp
cd lineos/m1/sp314-dsp 2>/dev/null || {
    echo "❌ G-008: sp314-dsp directory not found"
    exit 1
}

cargo test --test determinism -- --nocapture 2>&1
if [ ${PIPESTATUS[0]} -ne 0 ]; then
    echo "❌ G-008: Determinism check FAILED — golden hash mismatch"
    echo "   If intentional: update tests/determinism.rs"
    exit 1
fi

echo "✅ G-008: Determinism check passed"
