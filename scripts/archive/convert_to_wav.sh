#!/bin/bash

cd /tmp/fma_speech/

for f in *.mp3; do
    if [ -f "$f" ]; then
        filename=$(basename -- "$f")
        filename="${filename%.*}"
        echo "Converting $f to ${filename}.wav (30s, stereo)..."
        # -t 30: trim to 30s
        # -ac 2: force stereo
        # -y: overwrite
        ffmpeg -i "$f" -t 30 -ac 2 -ar 44100 "${filename}.wav" -y -loglevel error
        rm "$f"
    fi
done

echo "Conversion complete!"
