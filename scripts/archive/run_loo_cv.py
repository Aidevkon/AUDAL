#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np
import re
import concurrent.futures

def process_file(args):
    filename, thresh, bin_path = args
    cat = "music" if filename.startswith("music") else "speech"
    path = f"/tmp/diverse_corpus/{cat}/{filename}"
    
    try:
        out = subprocess.check_output([bin_path, path, thresh], text=True).strip()
        windows = []
        for line in out.split('\n'):
            if line.strip():
                windows.append(json.loads(line))
        cvs = [w["cv"] for w in windows]
        if cvs:
            return filename, cat, np.median(cvs)
    except Exception as e:
        pass
    return None

def find_best_threshold(data):
    # data is list of (file, cat, source, cv_med)
    if not data:
        return 0.0, 0.0
        
    sorted_data = sorted(data, key=lambda x: x[3])
    cvs = [x[3] for x in sorted_data]
    
    thresholds = []
    thresholds.append(cvs[0] - 0.01)
    for i in range(len(cvs) - 1):
        thresholds.append((cvs[i] + cvs[i+1]) / 2.0)
    thresholds.append(cvs[-1] + 0.01)
    
    best_t = 0.0
    best_acc = -1.0
    
    for t in thresholds:
        correct = 0
        for f, cat, src, cv_val in data:
            pred = "music" if cv_val <= t else "speech"
            if pred == cat:
                correct += 1
        acc = correct / len(data)
        if acc > best_acc:
            best_acc = acc
            best_t = t
            
    return best_t, best_acc

def run_analysis_for_thresh(thresh, passing, file_to_source, bin_path):
    print(f"\n============================================================")
    print(f"ANALYSIS FOR FLUX_THRESHOLD = {thresh}")
    print(f"============================================================")
    
    args_list = [(f, thresh, bin_path) for f in passing]
    dataset = []
    
    with concurrent.futures.ProcessPoolExecutor() as executor:
        results = executor.map(process_file, args_list)
        for res in results:
            if res is not None:
                filename, cat, cv_med = res
                source = file_to_source.get(filename, "unknown")
                dataset.append((filename, cat, source, cv_med))
                
    # 3. PER-SOURCE TABLE
    print("\n--- PER-SOURCE TABLE (FILE | CLASS | SOURCE | MEDIAN CV) ---")
    sources = sorted(list(set(x[2] for x in dataset)))
    for src in sources:
        src_files = [x for x in dataset if x[2] == src]
        print(f"Source: {src}")
        for f, cat, s, cv_val in src_files:
            print(f"  {f:<15} | {cat:<6} | {cv_val:.4f}")
            
    # 2. OVERLAPPING FILES (Specific to thresh)
    music_cvs = [x[3] for x in dataset if x[1] == "music"]
    speech_cvs = [x[3] for x in dataset if x[1] == "speech"]
    music_p75 = np.percentile(music_cvs, 75)
    speech_p25 = np.percentile(speech_cvs, 25)
    
    print(f"\n--- OVERLAPPING FILES AT {thresh} ---")
    print(f"Music p75 = {music_p75:.4f}, Speech p25 = {speech_p25:.4f}")
    print("Speech files below Music p75:")
    speech_below = [x for x in dataset if x[1] == "speech" and x[3] < music_p75]
    if not speech_below:
        print("  None")
    for f, cat, src, cv_val in speech_below:
        print(f"  {f:<15} | Source: {src:<35} | CV: {cv_val:.4f}")
        
    print("Music files above Speech p25:")
    music_above = [x for x in dataset if x[1] == "music" and x[3] > speech_p25]
    if not music_above:
        print("  None")
    for f, cat, src, cv_val in music_above:
        print(f"  {f:<15} | Source: {src:<35} | CV: {cv_val:.4f}")
        
    # 1. LEAVE-ONE-OUT ANALYSIS
    full_t, full_acc = find_best_threshold(dataset)
    print(f"\n--- LEAVE-ONE-OUT CV ANALYSIS ({thresh}) ---")
    print(f"Full Set Threshold: {full_t:.4f} (Accuracy: {full_acc*100:.2f}%)")
    
    loo_results = []
    for src in sources:
        subset = [x for x in dataset if x[2] != src]
        t, acc = find_best_threshold(subset)
        loo_results.append((src, t, acc))
        print(f"  Removed '{src}': Threshold = {t:.4f} (Acc on rest: {acc*100:.2f}%)")
        
    t_vals = [r[1] for r in loo_results]
    acc_vals = [r[2] for r in loo_results]
    
    min_t, max_t, spread = np.min(t_vals), np.max(t_vals), np.max(t_vals) - np.min(t_vals)
    min_acc, max_acc = np.min(acc_vals), np.max(acc_vals)
    
    print("\nLOO SUMMARY:")
    print(f"  Threshold Min:    {min_t:.4f}")
    print(f"  Threshold Max:    {max_t:.4f}")
    print(f"  Threshold Spread: {spread:.4f}")
    print(f"  Accuracy Range:   {min_acc*100:.1f}% - {max_acc*100:.1f}%")

def main():
    cmd = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/diverse_corpus", "--allow-mono"]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, text=True)
    
    passing = []
    for line in res.stdout.split("\n"):
        if line.startswith("PASS:"):
            filename = line.split("|")[0].replace("PASS:", "").strip()
            passing.append(filename)
            
    log_file = "/home/aidevcon/.gemini/antigravity/brain/ccd85db0-555a-4792-91b0-067c2811511c/.system_generated/tasks/task-19602.log"
    file_to_source = {}
    class_counts = {"music": 0, "speech": 0}
    with open(log_file, "r") as f:
        for line in f:
            m = re.search(r'\[(music|speech)\] Extracting 30s of .+? from (.+?)\.\.\.', line)
            if m:
                cat = m.group(1)
                ident = m.group(2)
                idx = class_counts[cat]
                wav_name = f"{cat}_{idx}.wav"
                file_to_source[wav_name] = ident
                class_counts[cat] += 1
                
    bin_path = "/home/aidevcon/Documents/creator-os/target/release/measure_variance"
    
    run_analysis_for_thresh("0.10", passing, file_to_source, bin_path)
    run_analysis_for_thresh("0.20", passing, file_to_source, bin_path)

if __name__ == "__main__":
    main()
