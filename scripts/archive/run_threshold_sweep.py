#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np
import concurrent.futures

def compute_percentiles(data_array):
    if not data_array:
        return 0, 0, 0
    return np.percentile(data_array, 25), np.median(data_array), np.percentile(data_array, 75)

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
        
        rates = [w["onset_count"] / 5.0 for w in windows]
        cvs = [w["cv"] for w in windows]
        robusts = [w["robust"] for w in windows]
        
        if cvs:
            return cat, rates, cvs, robusts, np.median(cvs), np.median(robusts)
    except Exception as e:
        pass
    return None

def main():
    cmd = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/diverse_corpus", "--allow-mono"]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, text=True)
    
    passing = []
    for line in res.stdout.split("\n"):
        if line.startswith("PASS:"):
            filename = line.split("|")[0].replace("PASS:", "").strip()
            passing.append(filename)
            
    bin_path = "/home/aidevcon/Documents/creator-os/target/release/measure_variance"
    thresholds = ["0.01", "0.05", "0.10", "0.20", "0.35", "0.50"]
    
    for thresh in thresholds:
        print(f"\n{'='*60}")
        print(f"THRESHOLD = {thresh}")
        print(f"{'='*60}")
        
        class_windows = {
            "music": {"onset_rate": [], "cv": [], "robust": []},
            "speech": {"onset_rate": [], "cv": [], "robust": []}
        }
        
        file_medians = {
            "music": {"cv": [], "robust": []},
            "speech": {"cv": [], "robust": []}
        }
        
        args_list = [(f, thresh, bin_path) for f in passing]
        with concurrent.futures.ProcessPoolExecutor() as executor:
            results = executor.map(process_file, args_list)
            
            for res in results:
                if res is not None:
                    cat, rates, cvs, robusts, med_cv, med_rob = res
                    class_windows[cat]["onset_rate"].extend(rates)
                    class_windows[cat]["cv"].extend(cvs)
                    class_windows[cat]["robust"].extend(robusts)
                    
                    file_medians[cat]["cv"].append(med_cv)
                    file_medians[cat]["robust"].append(med_rob)

        for cat in ["music", "speech"]:
            _, med_rate, _ = compute_percentiles(class_windows[cat]["onset_rate"])
            p25_cv, med_cv, p75_cv = compute_percentiles(class_windows[cat]["cv"])
            p25_rob, med_rob, p75_rob = compute_percentiles(class_windows[cat]["robust"])
            
            print(f"--- {cat.upper()} ---")
            print(f"Onset Rate/sec: Median = {med_rate:.2f}")
            print(f"CV:             p25={p25_cv:.3f}, Med={med_cv:.3f}, p75={p75_cv:.3f}")
            print(f"Robust:         p25={p25_rob:.3f}, Med={med_rob:.3f}, p75={p75_rob:.3f}")
            
        # Overlap
        music_cvs = np.array(file_medians["music"]["cv"])
        speech_cvs = np.array(file_medians["speech"]["cv"])
        
        if len(music_cvs) > 0 and len(speech_cvs) > 0:
            music_p75 = np.percentile(music_cvs, 75)
            speech_p25 = np.percentile(speech_cvs, 25)
            
            speech_below_music_p75 = np.mean(speech_cvs < music_p75)
            music_above_speech_p25 = np.mean(music_cvs > speech_p25)
            
            print("--- OVERLAP (CV) ---")
            print(f"Fraction of Speech files below Music p75 ({music_p75:.3f}): {speech_below_music_p75*100:.1f}%")
            print(f"Fraction of Music files above Speech p25 ({speech_p25:.3f}): {music_above_speech_p25*100:.1f}%")

if __name__ == "__main__":
    main()
