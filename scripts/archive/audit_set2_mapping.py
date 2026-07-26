#!/usr/bin/env python3
import os
import subprocess
import re
import numpy as np
import wave

def compute_10ms_frame_metrics(filepath):
    try:
        with wave.open(filepath, 'rb') as wf:
            n_channels = wf.getnchannels()
            sample_width = wf.getsampwidth()
            framerate = wf.getframerate()
            n_frames = wf.getnframes()
            raw_bytes = wf.readframes(n_frames)
        
        if sample_width == 2:
            data = np.frombuffer(raw_bytes, dtype=np.int16).astype(np.float64) / 32768.0
        elif sample_width == 4:
            data = np.frombuffer(raw_bytes, dtype=np.int32).astype(np.float64) / 2147483648.0
        else:
            return None

        if n_channels > 1:
            data = data.reshape(-1, n_channels).mean(axis=1)

        frame_len = int(framerate * 0.010)
        num_frames = len(data) // frame_len
        if num_frames == 0:
            return None

        frames = data[:num_frames * frame_len].reshape(num_frames, frame_len)
        rms = np.sqrt(np.mean(frames**2, axis=1))
        eps = 1e-12
        rms_db = 20.0 * np.log10(rms + eps)

        median_db = float(np.median(rms_db))
        p5_db = float(np.percentile(rms_db, 5))
        depth = median_db - p5_db

        thresh_25 = median_db - 25.0
        is_quiet = (rms_db < thresh_25)
        silence_fraction = float(np.mean(is_quiet))

        max_run = 0
        current_run = 0
        for q in is_quiet:
            if q:
                current_run += 1
                if current_run > max_run:
                    max_run = current_run
            else:
                current_run = 0
        longest_quiet_run_ms = float(max_run * 10.0)

        return {
            "median_db": median_db,
            "p5_db": p5_db,
            "depth_db": depth,
            "silence_fraction": silence_fraction,
            "longest_quiet_run_ms": longest_quiet_run_ms
        }
    except Exception as e:
        return None

# Map file index to source identifier
source_map = {}
for i in range(3, 12):
    source_map[f"speech_{i}.wav"] = "LibriVox Language Learning"
source_map["speech_12.wav"] = "JFK Centenary Podcast"
source_map["speech_13.wav"] = "George Saunders Podcast"
for i in range(14, 19):
    source_map[f"speech_{i}.wav"] = "Pale Blue Dot Podcast"
for i in range(19, 25):
    source_map[f"speech_{i}.wav"] = "Al-Quran Kashmiri"
for i in range(25, 30):
    source_map[f"speech_{i}.wav"] = "Art of War LibriVox"

gate_script = "/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py"
res_A = subprocess.run([gate_script, "/tmp/diverse_corpus", "--allow-mono"], stdout=subprocess.PIPE, text=True)
passing_speech = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "speech" in line]

print("EXACT MAPPING AND MEASUREMENTS FOR SET 2 (19 PASSING SPEECH FILES):")
print(f"{'FILE':<15} | {'REAL SOURCE NAME':<30} | {'MEDIAN dB':<9} | {'P5 dB':<8} | {'DEPTH dB':<8} | {'SIL FRAC':<8} | {'QUIET RUN':<9}")
print("-" * 105)

items_by_source = {}

for fn in sorted(passing_speech, key=lambda x: int(x.split("_")[1].split(".")[0])):
    path = f"/tmp/diverse_corpus/speech/{fn}"
    m = compute_10ms_frame_metrics(path)
    src = source_map.get(fn, "Unknown")
    print(f"{fn:<15} | {src:<30} | {m['median_db']:9.2f} | {m['p5_db']:8.2f} | {m['depth_db']:8.2f} | {m['silence_fraction']:8.4f} | {m['longest_quiet_run_ms']:7.0f} ms")
    
    if src not in items_by_source:
        items_by_source[src] = []
    items_by_source[src].append(m)

print("\n" + "="*80)
print("SET 2 RE-REPORTED BY REAL SOURCE GROUP (N = 19 passing files across 6 sources)")
print("="*80)
for src, items in items_by_source.items():
    depths = [x["depth_db"] for x in items]
    s_fracs = [x["silence_fraction"] for x in items]
    q_runs = [x["longest_quiet_run_ms"] for x in items]
    print(f"\nSOURCE: {src} (N = {len(items)})")
    print(f"  DEPTH (dB):            Min = {np.min(depths):5.2f} | Median = {np.median(depths):5.2f} | Max = {np.max(depths):5.2f}")
    print(f"  SILENCE FRACTION:      Min = {np.min(s_fracs):.4f} | Median = {np.median(s_fracs):.4f} | Max = {np.max(s_fracs):.4f}")
    print(f"  LONGEST QUIET RUN:     Min = {np.min(q_runs):5.1f} ms | Median = {np.median(q_runs):5.1f} ms | Max = {np.max(q_runs):5.1f} ms")
