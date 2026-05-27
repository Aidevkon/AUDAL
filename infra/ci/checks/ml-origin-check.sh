#!/bin/bash
# G-002: ML Origin Check
VIOLATIONS=0
ML_PATTERNS="ort::\|candle::\|tract::\|tch::\|tensorflow\|torch\|onnxruntime"

for dir in lineos/ pipelines/; do
    if grep -rn "$ML_PATTERNS" "$dir" 2>/dev/null \
       --include="*.rs" | grep -v "//\|test"; then
        echo "❌ G-002: ML library found in $dir"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
done

[ $VIOLATIONS -eq 0 ] && echo "✅ G-002: ML origin clean" || exit 1
