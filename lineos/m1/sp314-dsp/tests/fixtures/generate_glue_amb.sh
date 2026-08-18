#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

EXCERPT="../../../../../research/musdb-lab/excerpts/Al_James_-_Schoolboy_Facination/mixture.wav"
OUTPUT="glue_amb"

if [ ! -f "$EXCERPT" ]; then
    echo "Please ensure the musdb-lab excerpts are generated."
    exit 1
fi

rm -rf "$OUTPUT"
mkdir -p "$OUTPUT"

echo "Generating K=8 (Podcast mode) fixtures..."
cargo run --release --manifest-path ../../../../../research/musdb-lab/oracle_extract/Cargo.toml -- --semantic "$EXCERPT" "$OUTPUT"

mv "$OUTPUT/nmfd8" "$OUTPUT/nmfd8_k8"

echo "Generating K=11 (Music mode) fixtures..."
cargo run --release --manifest-path ../../../../../research/musdb-lab/oracle_extract/Cargo.toml -- --semantic --music "$EXCERPT" "$OUTPUT"

mv "$OUTPUT/nmfd8" "$OUTPUT/nmfd8_k11"

echo "Done generating fixtures in $OUTPUT"
