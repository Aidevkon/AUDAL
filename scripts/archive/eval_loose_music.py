#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np

def main():
    corpus_dir = "/tmp/loose_music"
    
    # 1. Run gate_corpus.py --allow-mono
    print("=== STEP 1: RUNNING GATE ON /tmp/loose_music ===")
    cmd_gate = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", corpus_dir, "--allow-mono"]
    res_gate = subprocess.run(cmd_gate, stdout=subprocess.PIPE, text=True)
    print(res_gate.stdout)
    
    passing_files = []
    for line in res_gate.stdout.split("\n"):
        if line.startswith("PASS:"):
            filename = line.split("|")[0].replace("PASS:", "").strip()
            passing_files.append(filename)
            
    print(f"\nTotal passing files: {len(passing_files)} / 10")
    
    # 2 & 3. Measure CV at FLUX_THRESHOLD = 0.20
    bin_path = "/home/aidevcon/Documents/creator-os/target/release/measure_variance"
    threshold = "0.20"
    full_set_thresh = 0.6406
    
    print("\n=== STEP 2 & 3: MEASURING CV AT FLUX_THRESHOLD 0.20 ===")
    print(f"{'FILE':<40} | {'CV MEDIAN':<12} | {'SIDE VS THRESHOLD (0.6406)'}")
    print("-" * 80)
    
    speech_side_count = 0
    music_side_count = 0
    
    for filename in sorted(passing_files):
        path = os.path.join(corpus_dir, "music", filename)
        try:
            out = subprocess.check_output([bin_path, path, threshold], text=True).strip()
            windows = []
            for line in out.split('\n'):
                if line.strip():
                    windows.append(json.loads(line))
            cvs = [w["cv"] for w in windows]
            if cvs:
                cv_med = np.median(cvs)
                side = "SPEECH SIDE (> 0.6406)" if cv_med > full_set_thresh else "MUSIC SIDE (<= 0.6406)"
                if cv_med > full_set_thresh:
                    speech_side_count += 1
                else:
                    music_side_count += 1
                print(f"{filename:<40} | {cv_med:<12.4f} | {side}")
        except Exception as e:
            print(f"Error evaluating {filename}: {e}")
            
    print("-" * 80)
    print(f"SUMMARY: {speech_side_count} / {len(passing_files)} files landed on the SPEECH SIDE.")
    print(f"SUMMARY: {music_side_count} / {len(passing_files)} files landed on the MUSIC SIDE.")

if __name__ == "__main__":
    main()
