#!/bin/bash
# Creator OS — Mass Ingestion Pipeline
# Usage: ./scripts/mass_ingest.sh /path/to/audio/folder
# Feeds multiple audio files through m0-daemon for corpus generation

set -e

AUDIO_DIR="${1:-.}"
DAEMON_URL="http://localhost:7401"
SUCCESS=0
FAILED=0

echo "=== Creator OS Mass Ingestion ==="
echo "Directory: $AUDIO_DIR"
echo "Target: $DAEMON_URL"
echo ""

# Check daemon is running
if ! curl -s "$DAEMON_URL/health" > /dev/null 2>&1; then
    echo "ERROR: m0-daemon not running. Start with:"
    echo "  cargo run --release -p m0d --bin m0d"
    exit 1
fi

# Process each wav file
for file in "$AUDIO_DIR"/*.wav "$AUDIO_DIR"/*.mp3 "$AUDIO_DIR"/*.flac; do
    [ -f "$file" ] || continue
    filename=$(basename "$file")
    
    echo -n "Processing: $filename ... "
    
    # Ensure absolute path
    abs_file=$(realpath "$file")
    
    response=$(curl -s -X POST "http://127.0.0.1:7402/master" \
        -H "Content-Type: application/json" \
        -d "{\"audioPath\":\"$abs_file\",\"presetId\":\"spotify\",\"flavourId\":\"broadcast\",\"intentTone\":0.5,\"intentDynamics\":0.5}" \
        --max-time 120 2>/dev/null)
    
    if echo "$response" | grep -q "blob_id"; then
        echo "OK"
        SUCCESS=$((SUCCESS + 1))
    else
        echo "FAILED"
        FAILED=$((FAILED + 1))
    fi
    
    sleep 0.5
done

echo ""
echo "=== Ingestion Complete ==="
echo "Success: $SUCCESS"
echo "Failed:  $FAILED"
echo "Corpus files: $(ls session_*.corpus.json 2>/dev/null | wc -l)"
echo ""

# Run pipeline if we have corpus data
if ls session_*.corpus.json > /dev/null 2>&1; then
    echo "Running Transition Extractor..."
    python3 tools/corpus/run_pipeline.py
    echo ""
    echo "Generated matrix:"
    cat aether/markov/generated/generated_voice_v2.rs | grep -A 7 "pub const"
else
    echo "No corpus files generated."
fi
