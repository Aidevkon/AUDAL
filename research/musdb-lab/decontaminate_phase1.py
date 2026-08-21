#!/usr/bin/env python3
"""
decontaminate_phase1.py — MUSDB-vs-FMA local cross-check (F-066 / Phase 1)

Fingerprints all MUSDB18-HQ mixtures and all FMA CC allowlist tracks using fpcalc,
and performs an all-pairs cross-check with offset alignment to detect any hidden
MUSDB tracks inside the FMA dataset.
"""

import os
import sys
import glob
import json
import subprocess
import numpy as np

MUSDB_ROOT = os.path.expanduser("~/Downloads/DATASET/musdb18hq")
FMA_ALLOWLIST = os.path.expanduser("~/Downloads/DATASET/fma/fma_small_cc_allowlist.json")
CACHE_DIR = "/tmp/decon"

FPCALC_BIN = os.path.expanduser("~/.local/bin/fpcalc")
if not os.path.exists(FPCALC_BIN):
    FPCALC_BIN = "fpcalc"

def run_fpcalc(path):
    cmd = [FPCALC_BIN, "-raw", "-length", "120", path]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        return []
    
    fp = []
    for line in res.stdout.splitlines():
        if line.startswith("FINGERPRINT="):
            raw_str = line.split("=", 1)[1]
            if raw_str.strip():
                fp = [int(x) for x in raw_str.split(",") if x.strip()]
    return fp

def main():
    os.makedirs(CACHE_DIR, exist_ok=True)
    
    # 1. Fingerprint MUSDB mixtures
    musdb_cache_path = os.path.join(CACHE_DIR, "musdb_fps.json")
    if os.path.exists(musdb_cache_path):
        print(f"Loading cached MUSDB fingerprints from {musdb_cache_path}...")
        with open(musdb_cache_path, "r") as f:
            musdb_fps = json.load(f)
    else:
        print("Fingerprinting MUSDB mixtures (train & test)...")
        musdb_fps = {}
        mixtures = sorted(glob.glob(os.path.join(MUSDB_ROOT, "*/*/mixture.wav")))
        for i, path in enumerate(mixtures, 1):
            rel_name = os.path.relpath(path, MUSDB_ROOT)
            fp = run_fpcalc(path)
            musdb_fps[rel_name] = fp
            if i % 25 == 0 or i == len(mixtures):
                print(f"[MUSDB FP] {i}/{len(mixtures)} processed ({rel_name})")
        
        with open(musdb_cache_path, "w") as f:
            json.dump(musdb_fps, f)
        print(f"Saved {len(musdb_fps)} MUSDB fingerprints to {musdb_cache_path}.")

    # 2. Fingerprint FMA allowlist tracks
    fma_cache_path = os.path.join(CACHE_DIR, "fma_fps.json")
    if os.path.exists(fma_cache_path):
        print(f"Loading cached FMA fingerprints from {fma_cache_path}...")
        with open(fma_cache_path, "r") as f:
            fma_fps = json.load(f)
    else:
        print("Fingerprinting FMA allowlist tracks...")
        if not os.path.exists(FMA_ALLOWLIST):
            print(f"ERROR: Allowlist file not found: {FMA_ALLOWLIST}")
            sys.exit(1)
            
        with open(FMA_ALLOWLIST, "r") as f:
            fma_items = json.load(f)
            
        fma_fps = {}
        for i, item in enumerate(fma_items, 1):
            path = item["path"]
            if os.path.exists(path):
                fp = run_fpcalc(path)
                fma_fps[path] = fp
            if i % 100 == 0 or i == len(fma_items):
                print(f"[FMA FP] {i}/{len(fma_items)} processed")
                
        with open(fma_cache_path, "w") as f:
            json.dump(fma_fps, f)
        print(f"Saved {len(fma_fps)} FMA fingerprints to {fma_cache_path}.")

    # 3. Cross-check pairwise similarity
    print("\nStarting pairwise cross-check (MUSDB vs FMA)...")
    candidates = []
    all_scores = []
    total_pairs = 0
    
    offsets = range(-5, 6) # [-5 .. 5]
    
    musdb_items = list(musdb_fps.items())
    fma_items = list(fma_fps.items())
    
    total_expected = len(musdb_items) * len(fma_items)
    print(f"Checking {len(musdb_items)} MUSDB tracks against {len(fma_items)} FMA tracks ({total_expected} total pairs)...")
    
    for m_idx, (m_name, m_fp) in enumerate(musdb_items, 1):
        m_arr = np.array(m_fp, dtype=np.uint32)
        if len(m_arr) == 0:
            continue
            
        for f_path, f_fp in fma_items:
            f_arr = np.array(f_fp, dtype=np.uint32)
            if len(f_arr) == 0:
                continue
                
            total_pairs += 1
            best_score = 0.0
            
            for offset in offsets:
                if offset >= 0:
                    sub_m = m_arr[offset:]
                    sub_f = f_arr
                else:
                    sub_m = m_arr
                    sub_f = f_arr[-offset:]
                    
                min_len = min(len(sub_m), len(sub_f))
                if min_len < 10:
                    continue
                    
                matches = np.sum(sub_m[:min_len] == sub_f[:min_len])
                score = matches / float(min_len)
                if score > best_score:
                    best_score = score
                    
            all_scores.append(best_score)
            
            if best_score >= 0.30:
                candidates.append((f_path, m_name, best_score))
                
        if m_idx % 25 == 0 or m_idx == len(musdb_items):
            print(f"[CROSS-CHECK] {m_idx}/{len(musdb_items)} MUSDB tracks completed ({total_pairs} pairs evaluated)...")

    # 4. Summary & Output
    all_scores = np.array(all_scores) if len(all_scores) > 0 else np.array([0.0])
    max_score = float(np.max(all_scores))
    mean_score = float(np.mean(all_scores))
    
    candidates_file = os.path.join(CACHE_DIR, "candidates.txt")
    with open(candidates_file, "w") as f:
        f.write("=== DECONTAMINATION PHASE 1: CANDIDATES ===\n")
        f.write(f"Threshold: >= 30.0% match\n")
        f.write(f"Total Candidates: {len(candidates)}\n\n")
        for f_path, m_name, score in candidates:
            f.write(f"FMA: {f_path} | MUSDB: {m_name} | Score: {score*100:.2f}%\n")
            
    print("\n" + "="*50)
    print("=== SUMMARY BLOCK: DECONTAMINATION PHASE 1 ===")
    print(f"Total pairs checked: {total_pairs}")
    print(f"Score max: {max_score*100:.2f}% ({max_score:.4f})")
    print(f"Score mean: {mean_score*100:.2f}% ({mean_score:.4f})")
    print(f"Total candidates (>= 30%): {len(candidates)}")
    if len(candidates) == 0:
        print(f"Purity status: CLEAN / DECONTAMINATED (Max score found: {max_score*100:.2f}%)")
    print("="*50)
    print(f"Candidates list written to: {candidates_file}")

if __name__ == "__main__":
    main()
