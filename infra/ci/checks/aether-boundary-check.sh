#!/bin/bash
# G-001: Aether Boundary Check
VIOLATIONS=0

if grep -rn "reqwest\|ureq\|hyper" aether/adapters/ 2>/dev/null; then
    echo "❌ G-001: Direct HTTP client in aether/adapters/"
    VIOLATIONS=$((VIOLATIONS+1))
fi

if grep -rn "-> Result<serde_json::Value" aether/adapters/ 2>/dev/null; then
    echo "❌ G-001: Raw serde_json::Value in adapter return type"
    VIOLATIONS=$((VIOLATIONS+1))
fi

for f in $(find aether/adapters/ -name "*.rs" 2>/dev/null); do
    if ! grep -q "validate_output" "$f"; then
        echo "❌ G-001: Missing validate_output() in $f"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
done

[ $VIOLATIONS -eq 0 ] && echo "✅ G-001: Aether boundary clean" || exit 1
