#!/usr/bin/env python3
import os
import sys
import json
import numpy as np
import subprocess
from concurrent.futures import ThreadPoolExecutor

def extract_window_features(filepath):
    cmd_var = ["/home/aidevcon/Documents/creator-os/target/release/measure_variance", filepath, "0.20"]
    res_var = subprocess.check_output(cmd_var, text=True)
    cv_by_win = {}
    for line in res_var.strip().split("\n"):
        if line.strip():
            try:
                d = json.loads(line)
                if "index" in d and "cv" in d:
                    cv_by_win[d["index"]] = d["cv"]
            except Exception: pass

    cmd_mfcc = ["/home/aidevcon/Documents/creator-os/target/release/measure_mfcc", filepath]
    res_mfcc = subprocess.check_output(cmd_mfcc, text=True)
    mfcc_data = json.loads(res_mfcc.strip())
    flux_by_win = {}
    for w in mfcc_data.get("windows", []):
        flux_by_win[w["window_index"]] = w["cepstral_flux"]

    win_indices = sorted(list(set(cv_by_win.keys()).intersection(set(flux_by_win.keys()))))
    windows = []
    for idx in win_indices:
        windows.append({
            "window_index": idx,
            "cv": cv_by_win[idx],
            "flux": flux_by_win[idx]
        })

    return {
        "path": filepath,
        "filename": os.path.basename(filepath),
        "windows": windows
    }

def compute_histogram(agreements):
    counts = [0] * 10
    for a in agreements:
        pct = a * 100.0
        if pct >= 100.0:
            idx = 9
        else:
            idx = int(pct // 10)
        counts[idx] += 1
    return counts

def calc_percentiles(vals):
    if not vals:
        return {"min": 0, "p25": 0, "median": 0, "p75": 0, "max": 0}
    return {
        "min": float(np.min(vals)),
        "p25": float(np.percentile(vals, 25)),
        "median": float(np.median(vals)),
        "p75": float(np.percentile(vals, 75)),
        "max": float(np.max(vals))
    }

def print_split_report(dataset, title, std_cv, std_fl, c_m_z, c_s_z):
    def classify_window(cv_val, flux_val):
        vec = np.array([cv_val / std_cv, flux_val / std_fl])
        d_m = np.linalg.norm(vec - c_m_z)
        d_s = np.linalg.norm(vec - c_s_z)
        return "speech" if d_s < d_m else "music"

    m_files = [x for x in dataset if x["label"] == "music"]
    s_files = [x for x in dataset if x["label"] == "speech"]

    m_win_corr = 0
    m_win_tot = 0
    m_agrees = []
    for x in m_files:
        corr = sum(1 for w in x["windows"] if classify_window(w["cv"], w["flux"]) == "music")
        tot = len(x["windows"])
        m_win_corr += corr
        m_win_tot += tot
        if tot > 0: m_agrees.append(corr / tot)

    s_win_corr = 0
    s_win_tot = 0
    s_agrees = []
    for x in s_files:
        corr = sum(1 for w in x["windows"] if classify_window(w["cv"], w["flux"]) == "speech")
        tot = len(x["windows"])
        s_win_corr += corr
        s_win_tot += tot
        if tot > 0: s_agrees.append(corr / tot)

    m_acc = m_win_corr / m_win_tot if m_win_tot > 0 else 0.0
    s_acc = s_win_corr / s_win_tot if s_win_tot > 0 else 0.0
    mean_acc = 0.5 * (m_acc + s_acc)

    pm = calc_percentiles(m_agrees)
    ps = calc_percentiles(s_agrees)

    hist_m = compute_histogram(m_agrees)
    hist_s = compute_histogram(s_agrees)

    print("\n" + "="*80)
    print(f"REPORT FOR {title}")
    print("="*80)
    print(f"  Music Window Accuracy:  {m_acc*100:6.2f}% ({m_win_corr}/{m_win_tot} windows)")
    print(f"  Speech Window Accuracy: {s_acc*100:6.2f}% ({s_win_corr}/{s_win_tot} windows)")
    print(f"  Unweighted Window Mean: {mean_acc*100:6.2f}%")

    print(f"\n  PER-FILE AGREEMENT FRACTION DISTRIBUTION (RIGHT ANSWERS / TOTAL WINDOWS IN FILE):")
    print(f"    MUSIC  (N={len(m_files)} files): Min={pm['min']*100:5.1f}% | p25={pm['p25']*100:5.1f}% | Median={pm['median']*100:5.1f}% | p75={pm['p75']*100:5.1f}% | Max={pm['max']*100:5.1f}%")
    print(f"    SPEECH (N={len(s_files)} files): Min={ps['min']*100:5.1f}% | p25={ps['p25']*100:5.1f}% | Median={ps['median']*100:5.1f}% | p75={ps['p75']*100:5.1f}% | Max={ps['max']*100:5.1f}%")

    print(f"\n  TEN-BIN HISTOGRAM OF PER-FILE AGREEMENT FRACTION:")
    bins_str = [" 0-10%", "10-20%", "20-30%", "30-40%", "40-50%", "50-60%", "60-70%", "70-80%", "80-90%", "90-100%"]
    print(f"    BIN        | MUSIC FILES ({len(m_files)}) | SPEECH FILES ({len(s_files)})")
    print("    " + "-"*50)
    for b_label, cm, cs in zip(bins_str, hist_m, hist_s):
        print(f"    {b_label:<10} | {cm:16d} | {cs:17d}")

def main():
    gate_script = "/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py"
    
    # 1. 130 Training Items
    train_items = []

    # User Music (95 files)
    genres = ['techno', 'acoustic', 'metal', 'pop']
    for g in genres:
        g_dir = f"/tmp/user_music/{g}"
        res_g = subprocess.run([gate_script, g_dir, "--allow-mono"], stdout=subprocess.PIPE, text=True)
        passing_g = [line.split("|")[0].replace("PASS:", "").strip() for line in res_g.stdout.split("\n") if line.startswith("PASS:")]
        for fn in passing_g:
            train_items.append((f"{g_dir}/music/{fn}", "music", "train"))

    # Classical (9 files)
    res_B = subprocess.run([gate_script, "/tmp/loose_music", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_classical = [line.split("|")[0].replace("PASS:", "").strip() for line in res_B.stdout.split("\n") if line.startswith("PASS:")]
    for fn in passing_classical:
        train_items.append((f"/tmp/loose_music/music/{fn}", "music", "train"))

    # Podcasts (26 files)
    with open("/tmp/podcast_results.json", "r") as f:
        pod_json = json.load(f)
    for item in pod_json["results"]:
        train_items.append((item["wav_path"], "speech", "train"))

    # 2. 40 Old Corpus Items
    old_items = []
    res_A = subprocess.run([gate_script, "/tmp/diverse_corpus", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_A_music = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "music" in line]
    passing_A_speech = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "speech" in line]
    for fn in passing_A_music: old_items.append((f"/tmp/diverse_corpus/music/{fn}", "music", "old"))
    for fn in passing_A_speech: old_items.append((f"/tmp/diverse_corpus/speech/{fn}", "speech", "old"))

    print("Extracting per-window features for 130 training files...")
    with ThreadPoolExecutor(max_workers=16) as pool:
        train_dataset = list(pool.map(lambda item: {**extract_window_features(item[0]), "label": item[1], "group": item[2]}, train_items))

    print("Extracting per-window features for 40 old corpus files...")
    with ThreadPoolExecutor(max_workers=16) as pool:
        old_dataset = list(pool.map(lambda item: {**extract_window_features(item[0]), "label": item[1], "group": item[2]}, old_items))

    # Compute centroids on all 130 training files
    m_130 = [x for x in train_dataset if x["label"] == "music"]
    s_130 = [x for x in train_dataset if x["label"] == "speech"]

    m_cv_file = [np.mean([w["cv"] for w in x["windows"]]) for x in m_130]
    s_cv_file = [np.mean([w["cv"] for w in x["windows"]]) for x in s_130]
    mu_m_cv = float(np.mean(m_cv_file))
    mu_s_cv = float(np.mean(s_cv_file))
    std_cv = float(np.sqrt(0.5 * (np.var(m_cv_file, ddof=1) + np.var(s_cv_file, ddof=1))))

    m_fl_file = [np.mean([w["flux"] for w in x["windows"]]) for x in m_130]
    s_fl_file = [np.mean([w["flux"] for w in x["windows"]]) for x in s_130]
    mu_m_fl = float(np.mean(m_fl_file))
    mu_s_fl = float(np.mean(s_fl_file))
    std_fl = float(np.sqrt(0.5 * (np.var(m_fl_file, ddof=1) + np.var(s_fl_file, ddof=1))))

    c_m_z = np.array([mu_m_cv / std_cv, mu_m_fl / std_fl])
    c_s_z = np.array([mu_s_cv / std_cv, mu_s_fl / std_fl])

    # Print reports for both sets
    print_split_report(train_dataset, "SET A: 130 TRAINING FILES (104 Music, 26 Speech)", std_cv, std_fl, c_m_z, c_s_z)
    print_split_report(old_dataset, "SET B: 40 OLD HELD-OUT CORPUS FILES (21 Music, 19 Speech)", std_cv, std_fl, c_m_z, c_s_z)

if __name__ == "__main__":
    main()
