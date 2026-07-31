#!/usr/bin/env bash
# render_variants.sh — Render the same music fixture through scalar (A) and
# spectral (B) reconstruction paths for A/B listening comparison.
#
# Mechanism: The scalar path is CURRENTLY driving the code (as requested).
# This script renders Variant A, applies spectral_switch.patch to test Variant B,
# and then reverts the patch to maintain the scalar path as HEAD.
#
# Usage:
#   cd /home/aidevcon/Documents/creator-os
#   bash research/ab-player/render_variants.sh
#
# Output:
#   /tmp/ab_variant_A_scalar.wav    (pre-switch code)
#   /tmp/ab_variant_B_spectral.wav  (6α spectral code)
#   Both loudness-matched via ffmpeg loudnorm (two-pass).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

FIXTURE="lineos/m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav"
OUT_A="/tmp/ab_variant_A_scalar.wav"
OUT_B="/tmp/ab_variant_B_spectral.wav"
OUT_A_NORM="/tmp/ab_variant_A_scalar_norm.wav"
OUT_B_NORM="/tmp/ab_variant_B_spectral_norm.wav"

if [ ! -f "$FIXTURE" ]; then
    echo "ERROR: Fixture $FIXTURE not found."
    exit 1
fi

echo "=== Variant A (scalar path — pre-switch code) ==="
cargo test -p m0d --test ab_render_full -- --nocapture --ignored render_ab_full_pipeline 2>&1 | tail -5
# The test hardcodes output to OUT_B, rename it:
mv "$OUT_B" "$OUT_A"
if [ ! -f "$OUT_A" ]; then
    echo "ERROR: $OUT_A was not created."
    exit 1
fi
echo "  Written: $OUT_A"

echo ""
echo "=== Applying 6α switch for spectral variant ==="
git apply -p0 research/ab-player/spectral_switch.patch

echo "=== Variant B (spectral path — 6α code) ==="
cargo test -p m0d --test ab_render_full -- --nocapture --ignored render_ab_full_pipeline 2>&1 | tail -5
if [ ! -f "$OUT_B" ]; then
    echo "ERROR: $OUT_B was not created."
    exit 1
fi
echo "  Written: $OUT_B"

echo ""
echo "=== Reverting 6α switch (keeping scalar as driver) ==="
git apply -p0 --reverse research/ab-player/spectral_switch.patch

echo ""
echo "=== Loudness matching (ffmpeg loudnorm two-pass) ==="
TARGET_LUFS="-16"

for src in "$OUT_A" "$OUT_B"; do
    base=$(basename "$src" .wav)
    norm="/tmp/${base}_norm.wav"
    # Pass 1: measure
    json=$(ffmpeg -nostdin -hide_banner -i "$src" -af loudnorm=I=${TARGET_LUFS}:print_format=json -f null - 2>&1 | grep -A 20 '"input_i"')
    input_i=$(echo "$json" | grep '"input_i"' | grep -o '[-0-9.]*')
    input_tp=$(echo "$json" | grep '"input_tp"' | grep -o '[-0-9.]*')
    input_lra=$(echo "$json" | grep '"input_lra"' | grep -o '[-0-9.]*')
    input_thresh=$(echo "$json" | grep '"input_thresh"' | grep -o '[-0-9.]*')
    # Pass 2: normalize
    ffmpeg -nostdin -hide_banner -y -i "$src" \
        -af "loudnorm=I=${TARGET_LUFS}:measured_I=${input_i}:measured_TP=${input_tp}:measured_LRA=${input_lra}:measured_thresh=${input_thresh}:linear=true" \
        "$norm" 2>/dev/null
    echo "  ${base}: measured ${input_i} LUFS → normalized to ${TARGET_LUFS} LUFS → $norm"
done

echo ""
echo "=== Final output ==="
echo "  Variant A (scalar):   $OUT_A_NORM"
echo "  Variant B (spectral): $OUT_B_NORM"
echo ""
echo "  Serve: cd research/ab-player && python3 -m http.server 8080"
echo "  Then open http://localhost:8080 in your browser."
