#!/bin/bash
# G-005: No serde_json::Value in Adapters
if grep -rn "serde_json::Value" aether/adapters/ 2>/dev/null \
   | grep -v "//"; then
    echo "❌ G-005: serde_json::Value in adapter I/O"
    exit 1
fi
echo "✅ G-005: No serde_json::Value in adapters"
