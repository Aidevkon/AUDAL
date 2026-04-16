#!/usr/bin/env bash
# ui-isolation-check.sh — full transitive dependency check
# Authority: Creator OS Amendment A-002 §8

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

UI_CRATES=("leptos" "dioxus" "dioxus-core" "dioxus-html" "leptos_dom" "leptos_macro")

CORE_CRATES=(
  "sp314-dsp"
  "lineos-telemetry"
  "lineos-rule-engine"
  "lineos-metadata"
  "lineos-insights"
  "m0d"
  "adapter-runtime"
)

VIOLATIONS=0

for crate in "${CORE_CRATES[@]}"; do
  DEPS=$(cargo tree -p "$crate" --prefix none 2>/dev/null \
         | awk '{print $1}' | sort -u)
  [ -z "$DEPS" ] && continue
  for ui_crate in "${UI_CRATES[@]}"; do
    if echo "$DEPS" | grep -q "^${ui_crate}$"; then
      echo "❌ VIOLATION: $crate → $ui_crate (transitive)"
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  done
  echo "  ✅ $crate — clean"
done

[ "$VIOLATIONS" -gt 0 ] && exit 1 || echo "✅ Core isolation: clean"
