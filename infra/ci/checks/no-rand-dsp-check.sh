#!/bin/bash
# G-009: No rand in DSP paths
VIOLATIONS=0

for dir in lineos/ pipelines/ aether/chaos/ aether/semantic/ \
           aether/mapping/; do
    if grep -rn "rand::\|thread_rng\|random()" "$dir" 2>/dev/null \
        --include="*.rs" | grep -v "//\|#\[cfg(test"; then
        echo "❌ G-009: rand usage in deterministic path: $dir"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
done

[ $VIOLATIONS -eq 0 ] && \
    echo "✅ G-009: No rand in DSP paths" || exit 1
