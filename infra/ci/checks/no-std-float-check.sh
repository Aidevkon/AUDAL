#!/bin/bash
# G-010: No std::f32 in DSP (libm only)
VIOLATIONS=0
PATTERNS="\.sqrt()\|\.log()\|\.log2()\|\.log10()\|\.powf(\|\.sin()\|\.cos()\|\.tan()\|\.exp()"

for file in $(find lineos/m1/sp314-dsp/src/ pipelines/ -name "*.rs" -not -path "*/src/bin/*"); do
    if awk '
        /^#\[cfg\(test\)\]/ { in_test=1; next }
        in_test && /^}/ { in_test=0; next }
        !in_test { print FILENAME ":" FNR ":" $0 }
    ' "$file" | grep "$PATTERNS" | grep -v "//\|libm"; then
        echo "❌ G-010: std::f32 method in DSP path: $file"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
done

[ $VIOLATIONS -eq 0 ] && echo "✅ G-010: libm only in DSP" || exit 1
