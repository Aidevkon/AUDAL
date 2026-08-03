#!/bin/bash
set -euo pipefail

# Ingest MUSDB18-HQ tracks and create 30s excerpts.
# Usage: ./ingest.sh /path/to/extracted/musdb18hq

if [ -z "${1:-}" ]; then
    echo "Usage: $0 /path/to/extracted/musdb18hq"
    exit 1
fi

DATASET_DIR="$1"
EXCERPT_DIR="$(dirname "$0")/excerpts"
MANIFEST="$(dirname "$0")/manifest.csv"

mkdir -p "$EXCERPT_DIR"

if [ ! -f "$MANIFEST" ]; then
    echo "track,excerpt_start,musdb_version,sha256_mix,sha256_vocals,sha256_bass,sha256_drums,sha256_other,rms_mix,rms_vocals,rms_bass,rms_drums,rms_other,roles,license_note" > "$MANIFEST"
fi

# Define the starter tracks (can be expanded later)
TRACKS=(
    "test/Al James - Schoolboy Facination"
    "test/Punkdisco - Oral Hygiene"
    "test/Forkupines - Semantics"
)

# 30 seconds excerpt starting at 00:30
START_TIME="00:00:30"
DURATION="30"

for TRACK in "${TRACKS[@]}"; do
    TRACK_PATH="$DATASET_DIR/$TRACK"
    if [ ! -d "$TRACK_PATH" ]; then
        echo "Skipping $TRACK (not found in dataset dir)"
        continue
    fi
    
    TRACK_NAME=$(basename "$TRACK")
    TRACK_SLUG=$(echo "$TRACK_NAME" | tr ' ' '_' | tr -cd 'A-Za-z0-9_-')
    OUT_DIR="$EXCERPT_DIR/$TRACK_SLUG"
    
    if [ -d "$OUT_DIR" ]; then
        echo "Skipping $TRACK (already excerpted)"
        continue
    fi
    
    echo "Processing $TRACK..."
    mkdir -p "$OUT_DIR"
    
    declare -A SHAS
    declare -A RMS
    
    for STEM in mixture vocals bass drums other; do
        ffmpeg -y -hide_banner -loglevel error \
            -ss "$START_TIME" -t "$DURATION" \
            -i "$TRACK_PATH/$STEM.wav" \
            -ar 48000 -c:a pcm_f32le \
            "$OUT_DIR/$STEM.wav"
            
        SHAS[$STEM]=$(sha256sum "$OUT_DIR/$STEM.wav" | awk '{print $1}')
        
        # Measure RMS of the generated excerpt
        RMS_VAL=$(ffmpeg -i "$OUT_DIR/$STEM.wav" -af volumedetect -f null - 2>&1 | grep "mean_volume:" | awk '{print $5}' || echo "-inf")
        RMS[$STEM]=$RMS_VAL
    done
    
    echo "\"$TRACK_NAME\",\"$START_TIME\",\"MUSDB18-HQ\",\"${SHAS[mixture]}\",\"${SHAS[vocals]}\",\"${SHAS[bass]}\",\"${SHAS[drums]}\",\"${SHAS[other]}\",\"${RMS[mixture]}\",\"${RMS[vocals]}\",\"${RMS[bass]}\",\"${RMS[drums]}\",\"${RMS[other]}\",\"vocals,bass,drums,other\",\"CC BY-NC-SA 4.0 or Educational\"" >> "$MANIFEST"
done

echo "Ingest complete."
