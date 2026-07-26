#!/bin/bash
set -e

mkdir -p /tmp/music_corpus
cd /tmp/music_corpus

echo "1. Downloading 15 FLAC music files from Internet Archive..."
# Try downloading small flac files using curl which handles URLs better if we encode them, or just use python to download reliably.
python3 -c '
import urllib.request
import json
import urllib.parse
import os

def download_files(format_str, target_ext, count_needed=15):
    url = f"https://archive.org/advancedsearch.php?q=mediatype:audio+AND+format:\"{format_str}\"+AND+subject:\"music\"&fl[]=identifier&sort[]=downloads+desc&rows=20&page=1&output=json"
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read())
    
    count = 0
    for doc in data["response"]["docs"]:
        if count >= count_needed: break
        ident = doc["identifier"]
        meta_url = f"https://archive.org/metadata/{ident}"
        with urllib.request.urlopen(meta_url) as meta_resp:
            meta_data = json.loads(meta_resp.read())
        
        for f in meta_data.get("files", []):
            name = f.get("name", "")
            if name.endswith(target_ext):
                print(f"Downloading {name} from {ident}...")
                dl_url = f"https://archive.org/download/{ident}/{urllib.parse.quote(name)}"
                try:
                    urllib.request.urlretrieve(dl_url, f"{ident}_{count}{target_ext}")
                    count += 1
                    if count >= count_needed: break
                except Exception as e:
                    print(f"Failed: {e}")

download_files("Flac", ".flac", 15)
print("2. Downloading 15 MP3 music files from Internet Archive...")
download_files("VBR MP3", ".mp3", 15)
'

echo "3. Converting all to 48kHz, stereo, pcm_s16le, 30s WAV..."
for f in *.mp3 *.flac; do
    [ -e "$f" ] || continue
    filename="${f%.*}"
    ffmpeg -i "$f" -t 30 -ar 48000 -ac 2 -c:a pcm_s16le "${filename}.wav" -y -loglevel error
    rm -f "$f"
done

echo "Music corpus generation complete!"
