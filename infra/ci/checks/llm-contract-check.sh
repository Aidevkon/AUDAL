#!/bin/bash
# G-003: LLM Contract Check
VIOLATIONS=0
FORBIDDEN="openai::\|anthropic::\|mistral_client\|llm_sdk"

if grep -rn "$FORBIDDEN" . --include="*.rs" \
    --exclude-dir="creator-os/shared/adapter-runtime" \
    2>/dev/null | grep -v "//"; then
    echo "❌ G-003: Direct LLM SDK call outside adapter-runtime"
    VIOLATIONS=$((VIOLATIONS+1))
fi

[ $VIOLATIONS -eq 0 ] && echo "✅ G-003: LLM contract clean" || exit 1
