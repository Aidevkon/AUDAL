#!/bin/bash
set -euo pipefail

STAGING_DIR="/tmp/fixture-factory"
mkdir -p "$STAGING_DIR"

echo "Downloading LibriVox sources..."
wget -q 'https://archive.org/download/winniethepoohversion6_2605_librivox/winniethepooh_00_milne_128kb.mp3' -O "$STAGING_DIR/voice1.mp3"
wget -q 'https://archive.org/download/basicbible01_02_2208_librivox/genesisexodus_01_bbe_128kb.mp3' -O "$STAGING_DIR/voice2.mp3"
wget -q 'https://archive.org/download/boy_scouts_susquehanna_2203_librivox/boyscoutssusquehanna_01_carter_128kb.mp3' -O "$STAGING_DIR/voice3.mp3"

echo "Downloading Free Music Archive (FMA) / Kevin MacLeod sources..."
wget -q 'https://archive.org/download/CognitiveDissonance-Section001/CognitiveDissonance001.mp3' -O "$STAGING_DIR/music1.mp3"
wget -q 'https://archive.org/download/DiscoLounge/DiscoLounge.mp3' -O "$STAGING_DIR/music2.mp3"
wget -q 'https://archive.org/download/garden-music-by-kevin-macleod/garden-music-by-kevin-macleod.mp3' -O "$STAGING_DIR/music3.mp3"

echo "Verifying formats..."
file "$STAGING_DIR"/*.mp3

echo "Done! Staging directory is $STAGING_DIR"
