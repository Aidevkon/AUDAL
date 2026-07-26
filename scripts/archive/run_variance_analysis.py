#!/usr/bin/env python3
import os
import subprocess
import json
import re
import numpy as np

def main():
    # 1. Get passing files from gate
    cmd = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/diverse_corpus", "--allow-mono"]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, text=True)
    
    passing = []
    for line in res.stdout.split("\n"):
        if line.startswith("PASS:"):
            filename = line.split("|")[0].replace("PASS:", "").strip()
            passing.append(filename)
            
    # 2. Map filename to source ident
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
                
    bin_path = "/home/aidevcon/Documents/creator-os/target/debug/measure_variance"
    
    print(f"{'FILE':<15} | {'SOURCE':<30} | {'ONSET_COUNT (min/med/max)':<25} | {'VAR_A (min/med/max)':<25} | {'CORR(onsets, var)'}")
    print("-" * 120)
    
    # Let's track the "exploding" ones and the low variance ones specifically
    # to answer Question 1 explicitly.
    exploding_files = []
    normal_files = []
    
    for filename in passing:
        cat = "music" if filename.startswith("music") else "speech"
        path = f"/tmp/diverse_corpus/{cat}/{filename}"
        
        try:
            out = subprocess.check_output([bin_path, path], text=True).strip()
            windows = []
            for line in out.split('\n'):
                if line.strip():
                    windows.append(json.loads(line))
        except Exception as e:
            print(f"Error measuring {filename}: {e}")
            continue
            
        onset_counts = [w["onset_count"] for w in windows]
        var_as = [w["variance_a"] for w in windows]
        
        if not onset_counts:
            continue
            
        oc_min, oc_med, oc_max = np.min(onset_counts), np.median(onset_counts), np.max(onset_counts)
        va_min, va_med, va_max = np.min(var_as), np.median(var_as), np.max(var_as)
        
        if len(onset_counts) > 1 and np.std(onset_counts) > 0 and np.std(var_as) > 0:
            corr = np.corrcoef(onset_counts, var_as)[0, 1]
        else:
            corr = float('nan')
            
        source = file_to_source.get(filename, "unknown")
        
        print(f"{filename:<15} | {source[:28]:<30} | {oc_min:3.0f}/{oc_med:3.0f}/{oc_max:3.0f}               | {va_min:7.1f}/{va_med:7.1f}/{va_max:7.1f}   | {corr:7.3f}")
        
        if va_med > 10000:
            exploding_files.append({"file": filename, "oc_med": oc_med, "va_med": va_med})
        elif va_med < 500:
            normal_files.append({"file": filename, "oc_med": oc_med, "va_med": va_med})

    print("\n=== SUMMARY FOR QUESTION 1 ===")
    print("Exploding Files (Median Var_A > 10,000):")
    for f in exploding_files:
        print(f"  {f['file']}: Median Onsets = {f['oc_med']:.1f}, Median Var_A = {f['va_med']:.1f}")
        
    print("Normal Files (Median Var_A < 500):")
    for f in normal_files[:5]: # just show a few
        print(f"  {f['file']}: Median Onsets = {f['oc_med']:.1f}, Median Var_A = {f['va_med']:.1f}")
        
if __name__ == "__main__":
    main()
