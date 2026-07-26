#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np
import re

def find_best_threshold(data):
    sorted_data = sorted(data, key=lambda x: x["median"])
    fluxes = [x["median"] for x in sorted_data]
    
    thresholds = []
    thresholds.append(fluxes[0] - 0.01)
    for i in range(len(fluxes) - 1):
        thresholds.append((fluxes[i] + fluxes[i+1]) / 2.0)
    thresholds.append(fluxes[-1] + 0.01)
    
    best_t = 0.0
    best_acc = -1.0
    
    for t in thresholds:
        correct = 0
        for item in data:
            pred = "speech" if item["median"] >= t else "music"
            if pred == item["class"]:
                correct += 1
        acc = correct / len(data)
        if acc > best_acc:
            best_acc = acc
            best_t = t
            
    return best_t, best_acc

def main():
    bin_path = "/home/aidevcon/Documents/creator-os/target/release/measure_mfcc"
    
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

    cmd_A = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/diverse_corpus", "--allow-mono"]
    res_A = subprocess.run(cmd_A, stdout=subprocess.PIPE, text=True)
    passing_A = []
    for line in res_A.stdout.split("\n"):
        if line.startswith("PASS:"):
            fn = line.split("|")[0].replace("PASS:", "").strip()
            passing_A.append(fn)

    cmd_B = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/loose_music", "--allow-mono"]
    res_B = subprocess.run(cmd_B, stdout=subprocess.PIPE, text=True)
    passing_B = []
    for line in res_B.stdout.split("\n"):
        if line.startswith("PASS:"):
            fn = line.split("|")[0].replace("PASS:", "").strip()
            passing_B.append(fn)

    all_windows = []
    dataset_A = []
    dataset_B = []

    for fn in sorted(passing_A):
        cat = "music" if fn.startswith("music") else "speech"
        path = f"/tmp/diverse_corpus/{cat}/{fn}"
        out = subprocess.check_output([bin_path, path], text=True).strip()
        data = json.loads(out)
        src = file_to_source.get(fn, "unknown")
        
        item = {
            "file": fn, "class": cat, "source": src,
            "min": data["flux_min"], "median": data["flux_median"], "max": data["flux_max"],
            "spread": data["flux_spread"], "windows": data["windows"]
        }
        dataset_A.append(item)
        
        for w in data["windows"]:
            all_windows.append({
                "file": fn, "class": cat, "source": src,
                "rms": w["rms"], "rms_db": 20.0 * np.log10(max(w["rms"], 1e-10)),
                "cepstral_flux": w["cepstral_flux"], "win_idx": w["window_index"]
            })

    for fn in sorted(passing_B):
        path = f"/tmp/loose_music/music/{fn}"
        out = subprocess.check_output([bin_path, path], text=True).strip()
        data = json.loads(out)
        item = {
            "file": fn, "class": "music", "source": "loose_classical",
            "min": data["flux_min"], "median": data["flux_median"], "max": data["flux_max"],
            "spread": data["flux_spread"], "windows": data["windows"]
        }
        dataset_B.append(item)
        
        for w in data["windows"]:
            all_windows.append({
                "file": fn, "class": "music", "source": "loose_classical",
                "rms": w["rms"], "rms_db": 20.0 * np.log10(max(w["rms"], 1e-10)),
                "cepstral_flux": w["cepstral_flux"], "win_idx": w["window_index"]
            })

    all_49 = dataset_A + dataset_B

    # 4. RE-REPORT PER-CLASS DISTRIBUTIONS & PER-FILE TABLE TOGETHER FROM SAME RUN
    print("============================================================")
    print("PER-CLASS CEPSTRAL FLUX DISTRIBUTIONS (FILE MEDIANS)")
    print("============================================================")
    
    music_medians_A = [x["median"] for x in dataset_A if x["class"] == "music"]
    speech_medians_A = [x["median"] for x in dataset_A if x["class"] == "speech"]
    classical_medians_B = [x["median"] for x in dataset_B]
    
    for name, arr in [("MUSIC (Set A, N=21)", music_medians_A), ("SPEECH (Set A, N=19)", speech_medians_A), ("LOOSE CLASSICAL (Set B, N=9)", classical_medians_B)]:
        mi, p25, med, p75, ma = np.min(arr), np.percentile(arr, 25), np.median(arr), np.percentile(arr, 75), np.max(arr)
        print(f"{name:<30}: Min={mi:.4f}, p25={p25:.4f}, Med={med:.4f}, p75={p75:.4f}, Max={ma:.4f}")

    # 1. THRESHOLD LOSO ON SET A
    print("\n============================================================")
    print("THRESHOLD LOSO ON SET A (SIMPLE SCALAR THRESHOLD)")
    print("============================================================")
    full_t, full_acc = find_best_threshold(dataset_A)
    
    sources_A = sorted(list(set(x["source"] for x in dataset_A)))
    loo_results = []
    
    for src in sources_A:
        subset = [x for x in dataset_A if x["source"] != src]
        t, acc = find_best_threshold(subset)
        loo_results.append((src, t, acc))
        print(f"  Removed '{src:<45}': Threshold = {t:.4f} (Acc on rest: {acc*100:.2f}%)")
        
    t_vals = [r[1] for r in loo_results]
    acc_vals = [r[2] for r in loo_results]
    min_t, max_t, spread_t = np.min(t_vals), np.max(t_vals), np.max(t_vals) - np.min(t_vals)
    min_acc, max_acc = np.min(acc_vals), np.max(acc_vals)
    
    print(f"\nFULL SET A THRESHOLD: {full_t:.4f} (Accuracy: {full_acc*100:.2f}%)")
    print(f"LOO SUMMARY:")
    print(f"  Threshold Min:    {min_t:.4f}")
    print(f"  Threshold Max:    {max_t:.4f}")
    print(f"  Threshold Spread: {spread_t:.4f}")
    print(f"  Accuracy Range:   {min_acc*100:.1f}% - {max_acc*100:.1f}%")

    # 2. PER-FILE TABLE (ALL 49 FILES GROUPED BY SOURCE)
    print("\n============================================================")
    print("PER-FILE TABLE (ALL 49 FILES GROUPED BY SOURCE)")
    print("============================================================")
    sources_all = sorted(list(set(x["source"] for x in all_49)))
    
    for src in sources_all:
        print(f"\nSource: {src}")
        src_files = [x for x in all_49 if x["source"] == src]
        for f in src_files:
            pred = "speech" if f["median"] >= full_t else "music"
            status = "CORRECT" if pred == f["class"] else "MISCLASSIFIED"
            print(f"  {f['file']:<40} | Class: {f['class']:<6} | Median Flux: {f['median']:.4f} | Pred: {pred:<6} | {status}")

    # WITHIN-FILE SPREAD WORST THREE
    print("\n============================================================")
    print("WITHIN-FILE SPREAD (WORST THREE FILES)")
    print("============================================================")
    sorted_by_spread = sorted(all_49, key=lambda x: x["spread"], reverse=True)
    for i in range(3):
        wf = sorted_by_spread[i]
        print(f"  #{i+1}: {wf['file']} ({wf['source']}) | Class: {wf['class']} | Min={wf['min']:.4f}, Med={wf['median']:.4f}, Max={wf['max']:.4f}, Spread={wf['spread']:.4f}")

    # QUIETEST WINDOWS
    print("\n============================================================")
    print("QUIETEST WINDOWS CHECK (10 LOWEST RMS WINDOWS)")
    print("============================================================")
    sorted_windows = sorted(all_windows, key=lambda x: x["rms"])
    print(f"{'RANK':<5} | {'FILE':<35} | {'CLASS':<6} | {'RMS (dBFS)':<12} | {'CEPSTRAL FLUX':<14} | {'SOURCE'}")
    print("-" * 105)
    for i in range(min(10, len(sorted_windows))):
        qw = sorted_windows[i]
        print(f"#{i+1:<4} | {qw['file']:<35} | {qw['class']:<6} | {qw['rms_db']:<12.2f} | {qw['cepstral_flux']:<14.4f} | {qw['source']}")

if __name__ == "__main__":
    main()
