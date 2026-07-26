#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np
import re

def compute_stats(data_array):
    if not data_array:
        return 0, 0, 0
    return np.min(data_array), np.median(data_array), np.max(data_array)

def compute_percentiles(data_array):
    if not data_array:
        return 0, 0, 0, 0, 0
    return np.min(data_array), np.percentile(data_array, 25), np.median(data_array), np.percentile(data_array, 75), np.max(data_array)

def main():
    cmd = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/diverse_corpus", "--allow-mono"]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, text=True)
    
    passing = []
    for line in res.stdout.split("\n"):
        if line.startswith("PASS:"):
            filename = line.split("|")[0].replace("PASS:", "").strip()
            passing.append(filename)
            
    bin_path = "/home/aidevcon/Documents/creator-os/target/debug/measure_variance"
    
    file_stats = {}
    class_windows_all = {"music": {"variance_a": [], "cv": [], "robust": []}, "speech": {"variance_a": [], "cv": [], "robust": []}}
    class_windows_guarded = {"music": {"variance_a": [], "cv": [], "robust": []}, "speech": {"variance_a": [], "cv": [], "robust": []}}
    
    print("=== PER-FILE WITHIN-FILE SPREAD ===")
    print(f"{'FILE':<15} | {'VAR_A (min/med/max)':<25} | {'CV (min/med/max)':<22} | {'ROBUST (min/med/max)':<22}")
    print("-" * 95)
    
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
            continue
            
        # All windows
        var_a = [w["variance_a"] for w in windows]
        cv = [w["cv"] for w in windows]
        robust = [w["robust"] for w in windows]
        
        # Guarded windows
        var_a_g = [w["variance_a"] for w in windows if w["onset_count"] >= 8]
        cv_g = [w["cv"] for w in windows if w["onset_count"] >= 8]
        robust_g = [w["robust"] for w in windows if w["onset_count"] >= 8]
        
        class_windows_all[cat]["variance_a"].extend(var_a)
        class_windows_all[cat]["cv"].extend(cv)
        class_windows_all[cat]["robust"].extend(robust)
        
        class_windows_guarded[cat]["variance_a"].extend(var_a_g)
        class_windows_guarded[cat]["cv"].extend(cv_g)
        class_windows_guarded[cat]["robust"].extend(robust_g)
        
        v_min, v_med, v_max = compute_stats(var_a)
        c_min, c_med, c_max = compute_stats(cv)
        r_min, r_med, r_max = compute_stats(robust)
        
        print(f"{filename:<15} | {v_min:7.1f}/{v_med:7.1f}/{v_max:7.1f}   | {c_min:6.3f}/{c_med:6.3f}/{c_max:6.3f} | {r_min:6.3f}/{r_med:6.3f}/{r_max:6.3f}")

    print("\n=== BETWEEN-CLASS PICTURE (ALL WINDOWS) ===")
    for metric in ["variance_a", "cv", "robust"]:
        print(f"--- {metric.upper()} ---")
        for cat in ["music", "speech"]:
            mi, p25, med, p75, ma = compute_percentiles(class_windows_all[cat][metric])
            print(f"{cat.upper()}: Min={mi:<8.3f} p25={p25:<8.3f} Med={med:<8.3f} p75={p75:<8.3f} Max={ma:<8.3f}")
            
    print("\n=== BETWEEN-CLASS PICTURE (GUARDED: ONSET_COUNT >= 8) ===")
    for metric in ["variance_a", "cv", "robust"]:
        print(f"--- {metric.upper()} ---")
        for cat in ["music", "speech"]:
            mi, p25, med, p75, ma = compute_percentiles(class_windows_guarded[cat][metric])
            print(f"{cat.upper()}: Min={mi:<8.3f} p25={p25:<8.3f} Med={med:<8.3f} p75={p75:<8.3f} Max={ma:<8.3f}")

if __name__ == "__main__":
    main()
