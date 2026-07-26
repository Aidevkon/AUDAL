#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np
import re
import concurrent.futures

def compute_cv(filepath):
    bin_path = "/home/aidevcon/Documents/creator-os/target/release/measure_variance"
    try:
        out = subprocess.check_output([bin_path, filepath, "0.20"], text=True).strip()
        windows = []
        for line in out.split('\n'):
            if line.strip():
                windows.append(json.loads(line))
        cvs = [w["cv"] for w in windows]
        return np.median(cvs) if cvs else 0.0
    except Exception as e:
        return 0.0

def compute_flux(filepath):
    bin_path = "/home/aidevcon/Documents/creator-os/target/release/measure_mfcc"
    try:
        out = subprocess.check_output([bin_path, filepath], text=True).strip()
        data = json.loads(out)
        return float(data["flux_median"])
    except Exception as e:
        return 0.0

def find_best_2d_and_thresholds(train_data):
    # train_data is list of dicts: {"file", "class", "source", "cv", "flux"}
    flux_vals = sorted(list(set(x["flux"] for x in train_data)))
    cv_vals = sorted(list(set(x["cv"] for x in train_data)))
    
    t1_candidates = [flux_vals[0] - 0.01] + [(flux_vals[i] + flux_vals[i+1])/2.0 for i in range(len(flux_vals)-1)] + [flux_vals[-1] + 0.01]
    t2_candidates = [cv_vals[0] - 0.01] + [(cv_vals[i] + cv_vals[i+1])/2.0 for i in range(len(cv_vals)-1)] + [cv_vals[-1] + 0.01]
    
    best_t1 = 0.0
    best_t2 = 0.0
    best_acc = -1.0
    
    for t1 in t1_candidates:
        for t2 in t2_candidates:
            correct = 0
            for item in train_data:
                pred = "speech" if (item["flux"] >= t1 and item["cv"] >= t2) else "music"
                if pred == item["class"]:
                    correct += 1
            acc = correct / len(train_data)
            if acc > best_acc:
                best_acc = acc
                best_t1 = t1
                best_t2 = t2
                
    return best_t1, best_t2, best_acc

def main():
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

    # 1. BUILD THE 2D TABLE FOR ALL 49 FILES
    dataset_A = []
    for fn in sorted(passing_A):
        cat = "music" if fn.startswith("music") else "speech"
        path = f"/tmp/diverse_corpus/{cat}/{fn}"
        cv_val = compute_cv(path)
        flux_val = compute_flux(path)
        src = file_to_source.get(fn, "unknown")
        dataset_A.append({"file": fn, "class": cat, "source": src, "cv": cv_val, "flux": flux_val})

    dataset_B = []
    for fn in sorted(passing_B):
        path = f"/tmp/loose_music/music/{fn}"
        cv_val = compute_cv(path)
        flux_val = compute_flux(path)
        dataset_B.append({"file": fn, "class": "music", "source": "loose_classical", "cv": cv_val, "flux": flux_val})

    all_49 = dataset_A + dataset_B

    print("============================================================")
    print("1. THE 2D TABLE (FILE | SOURCE | CLASS | CV | CEPSTRAL FLUX)")
    print("============================================================")
    print(f"{'FILE':<40} | {'SOURCE':<45} | {'CLASS':<6} | {'CV (0.20)':<10} | {'CEPSTRAL FLUX':<13}")
    print("-" * 125)
    for x in sorted(all_49, key=lambda item: (item["class"], item["source"])):
        print(f"{x['file']:<40} | {x['source']:<45} | {x['class']:<6} | {x['cv']:<10.4f} | {x['flux']:<13.4f}")

    # Correct record for 1D scalar threshold accuracy on Set A:
    # 1D flux threshold at 1.4782:
    scalar_t = 1.4782
    scalar_correct = 0
    print("\n--- RE-EVALUATING 1D FLUX ACCURACY AT 1.4782 ---")
    for x in dataset_A:
        pred = "speech" if x["flux"] >= scalar_t else "music"
        if pred == x["class"]:
            scalar_correct += 1
    print(f"True 1D Scalar Flux Accuracy at {scalar_t}: {scalar_correct}/{len(dataset_A)} ({scalar_correct/len(dataset_A)*100:.2f}%)")

    # 2. LEAVE-ONE-SOURCE-OUT (LOSO) WITH 2D CLASSIFIERS
    sources_A = sorted(list(set(x["source"] for x in dataset_A)))

    # 2A. Nearest Centroid on Z-scored (CV, flux)
    correct_centroid = 0
    misclassified_centroid = []

    for src in sources_A:
        train = [x for x in dataset_A if x["source"] != src]
        test = [x for x in dataset_A if x["source"] == src]

        tr_m_cv = [x["cv"] for x in train if x["class"] == "music"]
        tr_m_fl = [x["flux"] for x in train if x["class"] == "music"]
        tr_s_cv = [x["cv"] for x in train if x["class"] == "speech"]
        tr_s_fl = [x["flux"] for x in train if x["class"] == "speech"]

        m_m_cv, m_m_fl = np.mean(tr_m_cv), np.mean(tr_m_fl)
        m_s_cv, m_s_fl = np.mean(tr_s_cv), np.mean(tr_s_fl)

        std_cv = np.sqrt(((len(tr_m_cv)-1)*np.var(tr_m_cv, ddof=1) + (len(tr_s_cv)-1)*np.var(tr_s_cv, ddof=1)) / (len(train)-2))
        std_fl = np.sqrt(((len(tr_m_fl)-1)*np.var(tr_m_fl, ddof=1) + (len(tr_s_fl)-1)*np.var(tr_s_fl, ddof=1)) / (len(train)-2))

        c_m_z = np.array([m_m_cv / std_cv, m_m_fl / std_fl])
        c_s_z = np.array([m_s_cv / std_cv, m_s_fl / std_fl])

        for tf in test:
            z_tf = np.array([tf["cv"] / std_cv, tf["flux"] / std_fl])
            d_m = np.linalg.norm(z_tf - c_m_z)
            d_s = np.linalg.norm(z_tf - c_s_z)
            pred = "music" if d_m <= d_s else "speech"
            if pred == tf["class"]:
                correct_centroid += 1
            else:
                misclassified_centroid.append(tf)

    acc_centroid = (correct_centroid / len(dataset_A)) * 100.0

    # 2B. Simple AND Rule (predict SPEECH if flux >= t1 AND CV >= t2)
    correct_and = 0
    misclassified_and = []
    t1_list = []
    t2_list = []

    for src in sources_A:
        train = [x for x in dataset_A if x["source"] != src]
        test = [x for x in dataset_A if x["source"] == src]

        t1, t2, train_acc = find_best_2d_and_thresholds(train)
        t1_list.append(t1)
        t2_list.append(t2)

        src_correct = 0
        for tf in test:
            pred = "speech" if (tf["flux"] >= t1 and tf["cv"] >= t2) else "music"
            if pred == tf["class"]:
                src_correct += 1
                correct_and += 1
            else:
                misclassified_and.append(tf)

    acc_and = (correct_and / len(dataset_A)) * 100.0

    # Fit 2D AND thresholds on full Set A
    full_t1, full_t2, full_acc_and = find_best_2d_and_thresholds(dataset_A)

    print("\n============================================================")
    print("2. LEAVE-ONE-SOURCE-OUT (LOSO) 2D RESULTS")
    print("============================================================")
    print(f"2D Nearest Centroid LOSO Accuracy: {correct_centroid}/{len(dataset_A)} ({acc_centroid:.2f}%)")
    print(f"2D AND Rule LOSO Accuracy:         {correct_and}/{len(dataset_A)} ({acc_and:.2f}%)")
    print(f"Full Set A 2D AND Thresholds:      flux >= {full_t1:.4f} AND CV >= {full_t2:.4f} (Accuracy: {full_acc_and*100:.2f}%)")

    # 5. THRESHOLD STABILITY
    min_t1, max_t1, spread_t1 = np.min(t1_list), np.max(t1_list), np.max(t1_list) - np.min(t1_list)
    min_t2, max_t2, spread_t2 = np.min(t2_list), np.max(t2_list), np.max(t2_list) - np.min(t2_list)
    
    print("\n============================================================")
    print("5. 2D THRESHOLD STABILITY ACROSS FOLDS")
    print("============================================================")
    print(f"t1 (Flux Threshold): Min={min_t1:.4f}, Max={max_t1:.4f}, Spread={spread_t1:.4f}")
    print(f"t2 (CV Threshold):   Min={min_t2:.4f}, Max={max_t2:.4f}, Spread={spread_t2:.4f}")

    # 3. MISCLASSIFIED FILES IN 2D
    print("\n============================================================")
    print("3. MISCLASSIFIED FILES IN 2D AND RULE (full_t1={:.4f}, full_t2={:.4f})".format(full_t1, full_t2))
    print("============================================================")
    full_misclassified_and = []
    for x in dataset_A:
        pred = "speech" if (x["flux"] >= full_t1 and x["cv"] >= full_t2) else "music"
        if pred != x["class"]:
            full_misclassified_and.append(x)
            
    if not full_misclassified_and:
        print("  None! (100% Accuracy on Set A!)")
    for f in full_misclassified_and:
        print(f"  {f['file']:<40} | Source: {f['source']:<45} | Class: {f['class']:<6} | CV={f['cv']:.4f}, Flux={f['flux']:.4f}")

    # 4. WHERE DO THE 9 CLASSICAL LAND IN 2D?
    print("\n============================================================")
    print("4. WHERE DO THE 9 CLASSICAL FILES LAND IN 2D?")
    print("============================================================")
    b_music = 0
    b_speech = 0
    for bf in dataset_B:
        pred = "speech" if (bf["flux"] >= full_t1 and bf["cv"] >= full_t2) else "music"
        side = "MUSIC SIDE" if pred == "music" else "SPEECH SIDE"
        if pred == "music":
            b_music += 1
        else:
            b_speech += 1
        print(f"  {bf['file']:<40} | CV={bf['cv']:.4f}, Flux={bf['flux']:.4f} | {side}")
        
    print(f"\nClassical Result: {b_music}/{len(dataset_B)} fell on MUSIC SIDE, {b_speech}/{len(dataset_B)} fell on SPEECH SIDE.")

if __name__ == "__main__":
    main()
