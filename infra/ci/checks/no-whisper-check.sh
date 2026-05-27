#!/bin/bash
# G-006: No whisper-rs
if grep -rn "whisper.rs\|whisper-rs\|whisper_rs" . \
    --include="*.toml" --include="*.rs" 2>/dev/null \
    | grep -v "//\|deny.toml"; then
    echo "❌ G-006: whisper-rs (C++ FFI) detected"
    exit 1
fi
echo "✅ G-006: No whisper-rs"
