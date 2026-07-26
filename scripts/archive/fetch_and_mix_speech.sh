#!/bin/bash
set -e

DIR="/tmp/fma_speech"
rm -rf "$DIR"
mkdir -p "$DIR"

echo "1. Re-downloading FMA MP3s..."
cd /home/aidevcon/Documents/creator-os/scripts/dom_inspector
QUERIES=("speech" "dialogue" "narration" "voice" "interview" "podcast")
for q in "${QUERIES[@]}"; do
    node index.js "https://freemusicarchive.org/search?adv=1&music-filter-CC-attribution-only=true&quicksearch=$q" > temp_fma.json
    jq -r '.. | objects | select(.tag == "div" and (.attrs.class | strings | contains("play-item"))) | .attrs["data-track-info"]' temp_fma.json | head -n 3 > tracks.jsonl
    
    while IFS= read -r track; do
        if [ -z "$track" ] || [ "$track" == "null" ]; then continue; fi
        dl_url=$(echo "$track" | jq -r .playbackUrl)
        id=$(echo "$track" | jq -r .id)
        wget -q -O "$DIR/${id}_${q}.mp3" "$dl_url"
    done < tracks.jsonl
    rm -f temp_fma.json tracks.jsonl
done

echo "2. Downloading LibriSpeech FLACs (high quality lossless)..."
cd "$DIR"
wget -q -O dev-clean.tar.gz https://www.openslr.org/resources/12/dev-clean.tar.gz
# Extract only a few FLAC files to save time and space
tar -xzf dev-clean.tar.gz --wildcards "*.flac" || true

# Move all flac files to the main dir
find LibriSpeech -name "*.flac" -exec mv {} "$DIR/" \;
rm -rf LibriSpeech dev-clean.tar.gz

# Keep only 15 FLAC files to match the 15-20 MP3 files we have
ls *.flac | tail -n +16 | xargs -I {} rm {} 2>/dev/null || true

echo "3. Converting all sources to uniform WAV (48kHz, stereo, pcm_s16le, 30s)..."
# Convert MP3s
for f in *.mp3; do
    filename=$(basename -- "$f")
    filename="${filename%.*}"
    ffmpeg -i "$f" -t 30 -ar 48000 -ac 2 -c:a pcm_s16le "${filename}.wav" -y -loglevel error
    rm "$f"
done

# Convert FLACs
for f in *.flac; do
    filename=$(basename -- "$f")
    filename="${filename%.*}"
    ffmpeg -i "$f" -t 30 -ar 48000 -ac 2 -c:a pcm_s16le "${filename}.wav" -y -loglevel error
    rm "$f"
done

echo "All files uniformly converted! $(ls *.wav | wc -l) total speech tracks ready."
