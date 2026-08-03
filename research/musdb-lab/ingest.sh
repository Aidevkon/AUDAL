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
    echo "track,excerpt_start,musdb_version,sha256_mix,sha256_vocals,sha256_bass,sha256_drums,sha256_other,rms_mix,rms_vocals,rms_bass,rms_drums,rms_other,active_mix,active_vocals,active_bass,active_drums,active_other,roles,license_note" > "$MANIFEST"
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
    
    # We allow re-running if rms_envelope.csv is missing
    if [ -f "$OUT_DIR/rms_envelope.csv" ]; then
        if grep -q "\"$TRACK_NAME\"" "$MANIFEST"; then
            echo "Skipping $TRACK (already fully processed and in manifest)"
            continue
        fi
    fi
    
    echo "Processing $TRACK..."
    mkdir -p "$OUT_DIR"
    
    declare -A SHAS
    declare -A RMS
    
    for STEM in mixture vocals bass drums other; do
        # Extract audio if it doesn't exist
        if [ ! -f "$OUT_DIR/$STEM.wav" ]; then
            ffmpeg -y -hide_banner -loglevel error \
                -ss "$START_TIME" -t "$DURATION" \
                -i "$TRACK_PATH/$STEM.wav" \
                -ar 48000 -c:a pcm_f32le \
                "$OUT_DIR/$STEM.wav"
        fi
            
        SHAS[$STEM]=$(sha256sum "$OUT_DIR/$STEM.wav" | awk '{print $1}')
        
        # Measure mean RMS of the generated excerpt
        RMS_VAL=$(ffmpeg -i "$OUT_DIR/$STEM.wav" -af volumedetect -f null - 2>&1 | grep "mean_volume:" | awk '{print $5}' || echo "-inf")
        RMS[$STEM]=$RMS_VAL
        
        # Compute per-second RMS envelope (asetnsamples=48000 splits 48kHz audio into 1-sec chunks)
        if [ ! -f "$OUT_DIR/$STEM.rms" ] || [ ! -f "$OUT_DIR/rms_envelope.csv" ]; then
            ffmpeg -hide_banner -loglevel info -i "$OUT_DIR/$STEM.wav" \
                -af "asetnsamples=48000,astats=reset=1:metadata=1,ametadata=print:key=lavfi.astats.Overall.RMS_level" \
                -f null - 2>&1 | grep "lavfi.astats.Overall.RMS_level=" | awk -F= '{print $2}' | head -n 30 > "$OUT_DIR/$STEM.rms"
        fi
    done
    
    # Create the envelope CSV
    if [ ! -f "$OUT_DIR/rms_envelope.csv" ]; then
        echo "second,mixture,vocals,bass,drums,other" > "$OUT_DIR/rms_envelope.csv"
        paste -d, \
          <(seq 0 29) \
          "$OUT_DIR/mixture.rms" \
          "$OUT_DIR/vocals.rms" \
          "$OUT_DIR/bass.rms" \
          "$OUT_DIR/drums.rms" \
          "$OUT_DIR/other.rms" >> "$OUT_DIR/rms_envelope.csv"
    fi
      
    rm -f "$OUT_DIR"/*.rms
    
    # Compute active seconds using a quick python snippet
    # Prints 5 values separated by space: active_mix active_vocals active_bass active_drums active_other
    ACTIVE_SECS=$(python3 -c "
import csv, sys
def get_active(csv_path):
    stems = ['mixture', 'vocals', 'bass', 'drums', 'other']
    data = {s: [] for s in stems}
    with open(csv_path, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            for s in stems:
                val = row[s]
                data[s].append(-100.0 if val == '-inf' or val == '' else float(val))
    out = []
    for s in stems:
        max_v = max(data[s])
        out.append(str(sum(1 for v in data[s] if v >= max_v - 20.0)))
    print(' '.join(out))
get_active('$OUT_DIR/rms_envelope.csv')
")
    
    read -r ACT_MIX ACT_VOCALS ACT_BASS ACT_DRUMS ACT_OTHER <<< "$ACTIVE_SECS"
    
    # Only append to manifest if it's not already there
    if ! grep -q "\"$TRACK_NAME\"" "$MANIFEST"; then
        echo "\"$TRACK_NAME\",\"$START_TIME\",\"MUSDB18-HQ\",\"${SHAS[mixture]}\",\"${SHAS[vocals]}\",\"${SHAS[bass]}\",\"${SHAS[drums]}\",\"${SHAS[other]}\",\"${RMS[mixture]}\",\"${RMS[vocals]}\",\"${RMS[bass]}\",\"${RMS[drums]}\",\"${RMS[other]}\",\"$ACT_MIX\",\"$ACT_VOCALS\",\"$ACT_BASS\",\"$ACT_DRUMS\",\"$ACT_OTHER\",\"vocals,bass,drums,other\",\"CC BY-NC-SA 4.0 or Educational\"" >> "$MANIFEST"
    fi
done

echo "Ingest complete."
