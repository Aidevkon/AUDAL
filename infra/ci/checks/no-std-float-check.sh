#!/bin/bash
# G-010: No std::f32 in DSP (libm only)
VIOLATIONS=0
PATTERNS="\.sqrt()\|\.log()\|\.log2()\|\.log10()\|\.powf(\|\.sin()\|\.cos()\|\.tan()\|\.exp()"

for dir in lineos/m1/sp314-dsp/src/ pipelines/; do
    if grep -rn "$PATTERNS" "$dir" 2>/dev/null \
        --include="*.rs" | grep -v "//\|libm\|#\[cfg(test"; then
        echo "❌ G-010: std::f32 method in DSP path: $dir"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
done

[ $VIOLATIONS -eq 0 ] && echo "✅ G-010: libm only in DSP" || exit 1
