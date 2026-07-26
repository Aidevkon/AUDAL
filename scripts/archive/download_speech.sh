#!/bin/bash

mkdir -p /tmp/fma_speech
cd /home/aidevcon/Documents/creator-os/scripts/dom_inspector

# Queries to ensure diverse speech data (narration, dialogue, multiple languages/genders)
QUERIES=("speech" "dialogue" "narration" "voice" "interview" "podcast")

downloaded=0

for q in "${QUERIES[@]}"; do
    echo "Searching FMA for: $q"
    URL="https://freemusicarchive.org/search?adv=1&music-filter-CC-attribution-only=true&quicksearch=$q"
    
    # Run dom inspector and extract download URLs and track info
    node index.js "$URL" > temp_fma_$q.json
    
    # Parse out the data-track-info payload
    jq -r '.. | objects | select(.tag == "div" and (.attrs.class | strings | contains("play-item"))) | .attrs["data-track-info"]' temp_fma_$q.json | head -n 3 > tracks_$q.jsonl
    
    while IFS= read -r track; do
        if [ -z "$track" ] || [ "$track" == "null" ]; then
            continue
        fi
        
        dl_url=$(echo "$track" | jq -r .playbackUrl)
        title=$(echo "$track" | jq -r .title)
        id=$(echo "$track" | jq -r .id)
        
        echo "Downloading: $title ($dl_url)"
        
        # FMA download URLs often redirect to an MP3
        wget -q -O "/tmp/fma_speech/${id}_${q}.mp3" "$dl_url"
        
        downloaded=$((downloaded + 1))
        
    done < tracks_$q.jsonl
    
    rm temp_fma_$q.json tracks_$q.jsonl
done

echo "Successfully downloaded $downloaded speech tracks to /tmp/fma_speech/"
