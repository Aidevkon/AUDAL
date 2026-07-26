#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np
import re

def compute_pooled_std(music_vecs, speech_vecs):
    n_m = len(music_vecs)
    n_s = len(speech_vecs)
    
    var_m = np.var(music_vecs, axis=0, ddof=1) if n_m > 1 else np.zeros(13)
    var_s = np.var(speech_vecs, axis=0, ddof=1) if n_s > 1 else np.zeros(13)
    
    pooled_var = ((n_m - 1) * var_m + (n_s - 1) * var_s) / max(1, (n_m + n_s - 2))
    pooled_std = np.sqrt(pooled_var)
    pooled_std[pooled_std < 1e-6] = 1e-6
    return pooled_std

def main():
    bin_path = "/home/aidevcon/Documents/creator-os/target/release/measure_mfcc"
    
    # Load source map
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

    # Gate Set A
    cmd_A = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/diverse_corpus", "--allow-mono"]
    res_A = subprocess.run(cmd_A, stdout=subprocess.PIPE, text=True)
    passing_A = []
    for line in res_A.stdout.split("\n"):
        if line.startswith("PASS:"):
            fn = line.split("|")[0].replace("PASS:", "").strip()
            passing_A.append(fn)

    dataset_A = []
    for fn in sorted(passing_A):
        cat = "music" if fn.startswith("music") else "speech"
        path = f"/tmp/diverse_corpus/{cat}/{fn}"
        out = subprocess.check_output([bin_path, path], text=True).strip()
        data = json.loads(out)
        mfcc = np.array(data["mean_mfcc"])
        src = file_to_source.get(fn, "unknown")
        dataset_A.append({"file": fn, "class": cat, "source": src, "mfcc": mfcc})

    # Gate Set B
    cmd_B = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/loose_music", "--allow-mono"]
    res_B = subprocess.run(cmd_B, stdout=subprocess.PIPE, text=True)
    passing_B = []
    for line in res_B.stdout.split("\n"):
        if line.startswith("PASS:"):
            fn = line.split("|")[0].replace("PASS:", "").strip()
            passing_B.append(fn)

    dataset_B = []
    for fn in sorted(passing_B):
        path = f"/tmp/loose_music/music/{fn}"
        out = subprocess.check_output([bin_path, path], text=True).strip()
        data = json.loads(out)
        mfcc = np.array(data["mean_mfcc"])
        dataset_B.append({"file": fn, "class": "music", "source": "loose_classical", "mfcc": mfcc})

    music_vecs_A = np.array([x["mfcc"] for x in dataset_A if x["class"] == "music"])
    speech_vecs_A = np.array([x["mfcc"] for x in dataset_A if x["class"] == "speech"])
    
    mean_music_A = np.mean(music_vecs_A, axis=0)
    mean_speech_A = np.mean(speech_vecs_A, axis=0)
    pooled_std_A = compute_pooled_std(music_vecs_A, speech_vecs_A)

    # --- REPORT 1: CLASS MEAN VECTORS & POOLED STD ---
    print("=== REPORT 1: CLASS MEAN VECTORS & POOLED STD (SET A) ===")
    print(f"{'COEFF':<6} | {'MEAN MUSIC':<12} | {'MEAN SPEECH':<12} | {'POOLED STD':<12} | {'Z MUSIC':<10} | {'Z SPEECH':<10}")
    print("-" * 75)
    for i in range(13):
        zm = mean_music_A[i] / pooled_std_A[i]
        zs = mean_speech_A[i] / pooled_std_A[i]
        print(f"c{i:<5} | {mean_music_A[i]:<12.4f} | {mean_speech_A[i]:<12.4f} | {pooled_std_A[i]:<12.4f} | {zm:<10.2f} | {zs:<10.2f}")

    # --- REPORT 4: PER-COEFFICIENT SEPARATION ---
    print("\n=== REPORT 4: PER-COEFFICIENT SEPARATION (|mean_m - mean_s| / pooled_std) ===")
    sep = np.abs(mean_music_A - mean_speech_A) / pooled_std_A
    for i in range(13):
        bar = "*" * int(round(sep[i] * 10))
        print(f"c{i:<2}: {sep[i]:.4f} {bar}")

    # --- REPORT 2 & 3: LEAVE-ONE-SOURCE-OUT (LOSO) CLASSIFICATION ---
    sources_A = sorted(list(set(x["source"] for x in dataset_A)))
    print("\n=== REPORT 2 & 3: LEAVE-ONE-SOURCE-OUT (LOSO) CLASSIFICATION ===")
    
    total_correct = 0
    misclassified_files = []
    
    for src in sources_A:
        train_files = [x for x in dataset_A if x["source"] != src]
        test_files = [x for x in dataset_A if x["source"] == src]
        
        train_m = np.array([x["mfcc"] for x in train_files if x["class"] == "music"])
        train_s = np.array([x["mfcc"] for x in train_files if x["class"] == "speech"])
        
        c_m_train = np.mean(train_m, axis=0)
        c_s_train = np.mean(train_s, axis=0)
        p_std_train = compute_pooled_std(train_m, train_s)
        
        z_c_m = c_m_train / p_std_train
        z_c_s = c_s_train / p_std_train
        
        src_correct = 0
        for tf in test_files:
            z_f = tf["mfcc"] / p_std_train
            d_m = np.linalg.norm(z_f - z_c_m)
            d_s = np.linalg.norm(z_f - z_c_s)
            
            pred = "music" if d_m <= d_s else "speech"
            if pred == tf["class"]:
                src_correct += 1
                total_correct += 1
            else:
                misclassified_files.append({"file": tf["file"], "source": src, "actual": tf["class"], "pred": pred, "d_m": d_m, "d_s": d_s})
                
        acc_src = (src_correct / len(test_files)) * 100.0
        print(f"  Source '{src}': {src_correct}/{len(test_files)} correct ({acc_src:.1f}%)")

    overall_acc = (total_correct / len(dataset_A)) * 100.0
    print(f"\nOVERALL LOSO ACCURACY: {total_correct}/{len(dataset_A)} ({overall_acc:.2f}%)")

    print("\nMISCLASSIFIED FILES:")
    if not misclassified_files:
        print("  None!")
    for mf in misclassified_files:
        print(f"  {mf['file']:<15} | Source: {mf['source']:<35} | Actual: {mf['actual']:<6} | Pred: {mf['pred']:<6} (d_music={mf['d_m']:.2f}, d_speech={mf['d_s']:.2f})")

    # --- REPORT 5: SET B (9 CLASSICAL FILES) PLACEMENT ---
    print("\n=== REPORT 5: SET B (LOOSE CLASSICAL) PLACEMENT ===")
    print(f"{'FILE':<40} | {'DIST TO MUSIC':<14} | {'DIST TO SPEECH':<14} | {'SIDE'}")
    print("-" * 80)
    
    z_c_m_A = mean_music_A / pooled_std_A
    z_c_s_A = mean_speech_A / pooled_std_A
    
    b_music_count = 0
    b_speech_count = 0
    
    for bf in dataset_B:
        z_f = bf["mfcc"] / pooled_std_A
        d_m = np.linalg.norm(z_f - z_c_m_A)
        d_s = np.linalg.norm(z_f - z_c_s_A)
        
        side = "MUSIC SIDE" if d_m <= d_s else "SPEECH SIDE"
        if d_m <= d_s:
            b_music_count += 1
        else:
            b_speech_count += 1
            
        print(f"{bf['file']:<40} | {d_m:<14.4f} | {d_s:<14.4f} | {side}")
        
    print("-" * 80)
    print(f"Set B Result: {b_music_count} / {len(dataset_B)} fell on the MUSIC SIDE.")
    print(f"Set B Result: {b_speech_count} / {len(dataset_B)} fell on the SPEECH SIDE.")

if __name__ == "__main__":
    main()
