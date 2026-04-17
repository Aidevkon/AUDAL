#!/usr/bin/env bash
# ui-isolation-check.sh — full transitive dependency check
# Authority: Creator OS Amendment A-002 §8 · Amendment A-003 §10
#
# Check 1: DSP/inference crates must NOT import UI framework crates
# Check 2: Cockpit (UI) must NOT import PCM/audio-kernel crates (A-003 §5)

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

UI_FRAMEWORK_CRATES=("leptos" "dioxus" "dioxus-core" "dioxus-html" "leptos_dom" "leptos_macro")

# Phase 12A: Audio kernel crates — Cockpit must never import these (A-003 §5, §10)
AUDIO_KERNEL_CRATES=("xaak" "cpal" "ringbuf" "audiopus")

# DSP/inference crates — must not import UI framework crates
# Note: m0d is EXCLUDED — it is authorized to own xaak (A-003 §1)
DSP_CRATES=(
  "sp314-dsp"
  "lineos-telemetry"
  "lineos-rule-engine"
  "lineos-metadata"
  "lineos-insights"
  "adapter-runtime"
)

# UI crates — must not import audio kernel crates (A-003 §5)
UI_CRATES=(
  "stillair-cockpit"
)

VIOLATIONS=0

# ── Check 1: DSP crates must not import UI framework crates ───────────────────
echo "=== Check 1: DSP crates must not import UI framework crates ==="
for crate in "${DSP_CRATES[@]}"; do
  DEPS=$(cargo tree -p "$crate" --prefix none 2>/dev/null \
         | awk '{print $1}' | sort -u)
  [ -z "$DEPS" ] && continue
  for ui_crate in "${UI_FRAMEWORK_CRATES[@]}"; do
    if echo "$DEPS" | grep -q "^${ui_crate}$"; then
      echo "❌ VIOLATION: $crate → $ui_crate (UI framework in DSP crate)"
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  done
  echo "  ✅ $crate — clean"
done

# ── Check 2: UI crates must not import audio kernel crates ────────────────────
echo "=== Check 2: UI crates must not import audio kernel crates (A-003 §5) ==="
for crate in "${UI_CRATES[@]}"; do
  DEPS=$(cargo tree -p "$crate" --prefix none 2>/dev/null \
         | awk '{print $1}' | sort -u)
  [ -z "$DEPS" ] && { echo "  ⚠️  $crate — not found (skipping)"; continue; }
  for kern_crate in "${AUDIO_KERNEL_CRATES[@]}"; do
    if echo "$DEPS" | grep -q "^${kern_crate}$"; then
      echo "❌ VIOLATION: $crate → $kern_crate (audio kernel in UI crate — A-003 §5)"
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  done
  echo "  ✅ $crate — no audio kernel deps"
done

[ "$VIOLATIONS" -gt 0 ] && exit 1 || echo "✅ All isolation checks: clean"
