#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np
import re

def compute_pooled_std(music_feats, speech_feats):
    n_m = len(music_feats)
    n_s = len(speech_feats)
    dim = music_feats.shape[1]
    
    var_m = np.var(music_feats, axis=0, ddof=1) if n_m > 1 else np.zeros(dim)
    var_s = np.var(speech_feats, axis=0, ddof=1) if n_s > 1 else np.zeros(dim)
    
    pooled_var = ((n_m - 1) * var_m + (n_s - 1) * var_s) / max(1, (n_m + n_s - 2))
    pooled_std = np.sqrt(pooled_var)
    pooled_std[pooled_std < 1e-6] = 1e-6
    return pooled_std

def run_variant(variant_name, feature_fn, dataset_A, dataset_B, feature_names):
    print(f"\n============================================================")
    print(f"FEATURE SET: {variant_name} ({len(feature_names)} features)")
    print(f"============================================================")
    
    data_A = []
    for item in dataset_A:
        feats = feature_fn(item)
        data_A.append({"file": item["file"], "class": item["class"], "source": item["source"], "feats": feats})
        
    data_B = []
    for item in dataset_B:
        feats = feature_fn(item)
        data_B.append({"file": item["file"], "class": "music", "source": "loose_classical", "feats": feats})

    music_feats_A = np.array([x["feats"] for x in data_A if x["class"] == "music"])
    speech_feats_A = np.array([x["feats"] for x in data_A if x["class"] == "speech"])
    
    c_m_A = np.mean(music_feats_A, axis=0)
    c_s_A = np.mean(speech_feats_A, axis=0)
    p_std_A = compute_pooled_std(music_feats_A, speech_feats_A)
    
    # 2. Per-feature separation
    sep = np.abs(c_m_A - c_s_A) / p_std_A
    print("\n--- PER-FEATURE SEPARATION (|mean_m - mean_s| / pooled_std) ---")
    for name, s_val in zip(feature_names, sep):
        bar = "*" * int(round(s_val * 10))
        print(f"{name:<18}: {s_val:.4f} {bar}")

    # 1. Leave-One-Source-Out (LOSO)
    sources_A = sorted(list(set(x["source"] for x in data_A)))
    total_correct = 0
    misclassified = []
    
    print("\n--- LEAVE-ONE-SOURCE-OUT (LOSO) CLASSIFICATION ---")
    for src in sources_A:
        train_files = [x for x in data_A if x["source"] != src]
        test_files = [x for x in data_A if x["source"] == src]
        
        train_m = np.array([x["feats"] for x in train_files if x["class"] == "music"])
        train_s = np.array([x["feats"] for x in train_files if x["class"] == "speech"])
        
        c_m_tr = np.mean(train_m, axis=0)
        c_s_tr = np.mean(train_s, axis=0)
        p_std_tr = compute_pooled_std(train_m, train_s)
        
        z_c_m = c_m_tr / p_std_tr
        z_c_s = c_s_tr / p_std_tr
        
        src_correct = 0
        for tf in test_files:
            z_f = tf["feats"] / p_std_tr
            d_m = np.linalg.norm(z_f - z_c_m)
            d_s = np.linalg.norm(z_f - z_c_s)
            
            pred = "music" if d_m <= d_s else "speech"
            if pred == tf["class"]:
                src_correct += 1
                total_correct += 1
            else:
                misclassified.append({"file": tf["file"], "source": src, "actual": tf["class"], "pred": pred, "d_m": d_m, "d_s": d_s})
                
        acc_src = (src_correct / len(test_files)) * 100.0
        print(f"  Source '{src:<45}': {src_correct}/{len(test_files)} ({acc_src:.1f}%)")
        
    overall_acc = (total_correct / len(data_A)) * 100.0
    print(f"\nOVERALL LOSO ACCURACY: {total_correct}/{len(data_A)} ({overall_acc:.2f}%)")
    
    # Misclassified files
    print("\n--- MISCLASSIFIED FILES ---")
    if not misclassified:
        print("  None!")
    for mf in misclassified:
        print(f"  {mf['file']:<15} | Source: {mf['source']:<35} | Actual: {mf['actual']:<6} | Pred: {mf['pred']:<6} (d_m={mf['d_m']:.2f}, d_s={mf['d_s']:.2f})")

    # 3. Where 9 classical files land
    print("\n--- SET B (LOOSE CLASSICAL) PLACEMENT ---")
    z_c_m_A = c_m_A / p_std_A
    z_c_s_A = c_s_A / p_std_A
    
    b_music = 0
    b_speech = 0
    for bf in data_B:
        z_f = bf["feats"] / p_std_A
        d_m = np.linalg.norm(z_f - z_c_m_A)
        d_s = np.linalg.norm(z_f - z_c_s_A)
        side = "MUSIC SIDE" if d_m <= d_s else "SPEECH SIDE"
        if d_m <= d_s:
            b_music += 1
        else:
            b_speech += 1
        print(f"  {bf['file']:<42} | d_m={d_m:.4f} | d_s={d_s:.4f} | {side}")
        
    print(f"\nSet B Result: {b_music}/{len(data_B)} fell on MUSIC SIDE, {b_speech}/{len(data_B)} fell on SPEECH SIDE.")
    return overall_acc, b_music

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

    # Load data for Set A
    dataset_A = []
    for fn in sorted(passing_A):
        cat = "music" if fn.startswith("music") else "speech"
        path = f"/tmp/diverse_corpus/{cat}/{fn}"
        out = subprocess.check_output([bin_path, path], text=True).strip()
        data = json.loads(out)
        src = file_to_source.get(fn, "unknown")
        dataset_A.append({
            "file": fn,
            "class": cat,
            "source": src,
            "window_mean": np.array(data["window_mean"]),
            "frame_std": np.array(data["frame_std"]),
            "cepstral_flux": float(data["cepstral_flux"])
        })

    # Load data for Set B
    dataset_B = []
    for fn in sorted(passing_B):
        path = f"/tmp/loose_music/music/{fn}"
        out = subprocess.check_output([bin_path, path], text=True).strip()
        data = json.loads(out)
        dataset_B.append({
            "file": fn,
            "class": "music",
            "source": "loose_classical",
            "window_mean": np.array(data["window_mean"]),
            "frame_std": np.array(data["frame_std"]),
            "cepstral_flux": float(data["cepstral_flux"])
        })

    # REPORT SCALAR CEPSTRAL FLUX DISTRIBUTIONS
    flux_music_A = [x["cepstral_flux"] for x in dataset_A if x["class"] == "music"]
    flux_speech_A = [x["cepstral_flux"] for x in dataset_A if x["class"] == "speech"]
    flux_classical_B = [x["cepstral_flux"] for x in dataset_B]
    
    print("============================================================")
    print("CEPSTRAL FLUX DISTRIBUTION AS A SINGLE SCALAR")
    print("============================================================")
    for name, arr in [("MUSIC (Set A)", flux_music_A), ("SPEECH (Set A)", flux_speech_A), ("LOOSE CLASSICAL (Set B)", flux_classical_B)]:
        mi, p25, med, p75, ma = np.min(arr), np.percentile(arr, 25), np.median(arr), np.percentile(arr, 75), np.max(arr)
        print(f"{name:<25}: Min={mi:.4f}, p25={p25:.4f}, Med={med:.4f}, p75={p75:.4f}, Max={ma:.4f}")
        
    overlap_flux_speech_below_music_p75 = np.mean(np.array(flux_speech_A) < np.percentile(flux_music_A, 75))
    overlap_flux_music_above_speech_p25 = np.mean(np.array(flux_music_A) > np.percentile(flux_speech_A, 25))
    print(f"Flux Overlap: Speech below Music p75 ({np.percentile(flux_music_A, 75):.4f}): {overlap_flux_speech_below_music_p75*100:.1f}%")
    print(f"Flux Overlap: Music above Speech p25 ({np.percentile(flux_speech_A, 25):.4f}): {overlap_flux_music_above_speech_p25*100:.1f}%")
    print("============================================================")

    # FEATURE SET A: frame_std only (12 features)
    fn_A = lambda item: item["frame_std"]
    names_A = [f"frame_std_c{i}" for i in range(1, 13)]
    accA, bA = run_variant("SET A: frame_std only", fn_A, dataset_A, dataset_B, names_A)

    # FEATURE SET B: cepstral_flux only (1 feature)
    fn_B = lambda item: np.array([item["cepstral_flux"]])
    names_B = ["cepstral_flux"]
    accB, bB = run_variant("SET B: cepstral_flux only", fn_B, dataset_A, dataset_B, names_B)

    # FEATURE SET C: frame_std + cepstral_flux + window_mean (25 features)
    fn_C = lambda item: np.concatenate([item["frame_std"], [item["cepstral_flux"]], item["window_mean"]])
    names_C = [f"frame_std_c{i}" for i in range(1, 13)] + ["cepstral_flux"] + [f"window_mean_c{i}" for i in range(1, 13)]
    accC, bC = run_variant("SET C: frame_std + cepstral_flux + window_mean", fn_C, dataset_A, dataset_B, names_C)

    print("\n============================================================")
    print("FINAL SUMMARY OF DECISION METRICS")
    print("============================================================")
    print(f"Feature Set A (frame_std only 12D):    LOSO Acc = {accA:.2f}%, Classical Music Side = {bA}/9")
    print(f"Feature Set B (cepstral_flux only 1D): LOSO Acc = {accB:.2f}%, Classical Music Side = {bB}/9")
    print(f"Feature Set C (Combined 25D):          LOSO Acc = {accC:.2f}%, Classical Music Side = {bC}/9")

if __name__ == "__main__":
    main()
