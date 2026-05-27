#!/bin/bash
# G-004: Layer Isolation Check
VIOLATIONS=0

# Aether must not import lineos internals
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
