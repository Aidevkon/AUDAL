#!/usr/bin/env python3
import os
import sys
import json
import numpy as np
import subprocess
from concurrent.futures import ThreadPoolExecutor

def get_metadata(fpath):
    cmd = ['ffprobe', '-v', 'quiet', '-print_format', 'json', '-show_format', '-show_streams', fpath]
    try:
        res = subprocess.check_output(cmd, text=True)
        info = json.loads(res)
        fmt = info.get('format', {})
        tags = fmt.get('tags', {})
        artist = tags.get('artist', tags.get('ARTIST', ''))
        album = tags.get('album', tags.get('ALBUM', ''))
        title = tags.get('title', tags.get('TITLE', os.path.basename(fpath)))
        if artist and album: src = f"{artist} - {album}"
        elif artist: src = artist
        else: src = os.path.basename(fpath).split('.')[0]
        return {"artist": artist, "album": album, "title": title, "source": src}
    except Exception:
        return {"artist": "", "album": "", "title": os.path.basename(fpath), "source": os.path.basename(fpath).split('.')[0]}

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
            except Exception:
                pass

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

def main():
    gate_script = "/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py"
    
    all_items = []

    # 1. 95 User Music
    genres = ['techno', 'acoustic', 'metal', 'pop']
    for g in genres:
        g_dir = f"/tmp/user_music/{g}"
        res_g = subprocess.run([gate_script, g_dir, "--allow-mono"], stdout=subprocess.PIPE, text=True)
        passing_g = [line.split("|")[0].replace("PASS:", "").strip() for line in res_g.stdout.split("\n") if line.startswith("PASS:")]
        for fn in passing_g:
            p = f"{g_dir}/music/{fn}"
            all_items.append((p, "music", "user_music", fn))

    # 2. 9 Classical
    res_B = subprocess.run([gate_script, "/tmp/loose_music", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_classical = [line.split("|")[0].replace("PASS:", "").strip() for line in res_B.stdout.split("\n") if line.startswith("PASS:")]
    for fn in passing_classical:
        p = f"/tmp/loose_music/music/{fn}"
        all_items.append((p, "music", "classical", fn))

    # 3. 26 Podcasts
    with open("/tmp/podcast_results.json", "r") as f:
        pod_json = json.load(f)
    for item in pod_json["results"]:
        p = item["wav_path"]
        all_items.append((p, "speech", "podcast", item["wav_name"]))

    # 4. 40 Old Corpus
    res_A = subprocess.run([gate_script, "/tmp/diverse_corpus", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_A_music = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "music" in line]
    passing_A_speech = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "speech" in line]
    for fn in passing_A_music:
        all_items.append((f"/tmp/diverse_corpus/music/{fn}", "music", "old_corpus", fn))
    for fn in passing_A_speech:
        all_items.append((f"/tmp/diverse_corpus/speech/{fn}", "speech", "old_corpus", fn))

    print(f"Extracting per-window features for all {len(all_items)} files...")
    with ThreadPoolExecutor(max_workers=16) as pool:
        raw_results = list(pool.map(lambda item: {**extract_window_features(item[0]), "label": item[1], "group": item[2]}, all_items))

    # 4. CONFIRM CENTROID FILE LIST FIRST
    train_130_results = [x for x in raw_results if x["group"] != "old_corpus"]
    
    m_130 = [x for x in train_130_results if x["label"] == "music"]
    s_130 = [x for x in train_130_results if x["label"] == "speech"]

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

    print("\n" + "="*80)
    print("4. CENTROID FILE LIST & LOCKED MODEL PARAMETERS")
    print("="*80)
    print(f"  Training Files Used: 130 files total (104 Music, 26 Speech)")
    print(f"  Speech Files Included: ALL 26 Podcasts (speech_0.wav to speech_26.wav, except speech_14 which failed extraction)")
    print(f"  Music Files Included: 95 User Music files + 9 Loose Classical files")
    print(f"  Locked Centroids: ")
    print(f"    CV:   Music Mean = {mu_m_cv:.4f}, Speech Mean = {mu_s_cv:.4f}, Pooled Std = {std_cv:.4f}")
    print(f"    Flux: Music Mean = {mu_m_fl:.4f}, Speech Mean = {mu_s_fl:.4f}, Pooled Std = {std_fl:.4f}")

    c_m_z = np.array([mu_m_cv / std_cv, mu_m_fl / std_fl])
    c_s_z = np.array([mu_s_cv / std_cv, mu_s_fl / std_fl])

    def classify_window(cv_val, flux_val):
        vec = np.array([cv_val / std_cv, flux_val / std_fl])
        d_m = np.linalg.norm(vec - c_m_z)
        d_s = np.linalg.norm(vec - c_s_z)
        return "speech" if d_s < d_m else "music"

    # 1. WINDOW-LEVEL ACCURACY & PER-FILE AGREEMENT DISTRIBUTION
    m_win_correct = 0
    m_win_total = 0
    s_win_correct = 0
    s_win_total = 0

    per_file_agreements_m = []
    per_file_agreements_s = []

    file_window_spreads_cv = []
    file_window_spreads_fl = []

    smoothed_results = []

    for file_res in raw_results:
        true_label = file_res["label"]
        wins = file_res["windows"]
        if not wins: continue

        win_preds = []
        cv_arr = []
        fl_arr = []

        for w in wins:
            p = classify_window(w["cv"], w["flux"])
            win_preds.append(p)
            cv_arr.append(w["cv"])
            fl_arr.append(w["flux"])

            if true_label == "music":
                m_win_total += 1
                if p == "music": m_win_correct += 1
            else:
                s_win_total += 1
                if p == "speech": s_win_correct += 1

        n_corr = sum(1 for p in win_preds if p == true_label)
        agree_frac = n_corr / len(win_preds)

        if true_label == "music":
            per_file_agreements_m.append(agree_frac)
        else:
            per_file_agreements_s.append(agree_frac)

        # 2. Per-window feature spread within file (p75 - p25)
        p75_cv, p25_cv = np.percentile(cv_arr, 75), np.percentile(cv_arr, 25)
        p75_fl, p25_fl = np.percentile(fl_arr, 75), np.percentile(fl_arr, 25)
        file_window_spreads_cv.append(p75_cv - p25_cv)
        file_window_spreads_fl.append(p75_fl - p25_fl)

        # 3. MAJORITY VOTE OVER 5-WINDOW SPAN
        smoothed_preds = []
        N_w = len(win_preds)
        for i in range(N_w):
            w_start = max(0, i - 2)
            w_end = min(N_w, i + 3)
            sub_preds = win_preds[w_start:w_end]
            m_count = sub_preds.count("music")
            s_count = sub_preds.count("speech")
            s_pred = "speech" if s_count > m_count else "music"
            smoothed_preds.append(s_pred)

        n_smooth_corr = sum(1 for p in smoothed_preds if p == true_label)
        smooth_agree_frac = n_smooth_corr / len(smoothed_preds)
        file_maj_vote_pred = "speech" if smoothed_preds.count("speech") > smoothed_preds.count("music") else "music"

        smoothed_results.append({
            "filename": file_res["filename"],
            "label": true_label,
            "group": file_res["group"],
            "raw_agree_frac": agree_frac,
            "smooth_agree_frac": smooth_agree_frac,
            "raw_preds": win_preds,
            "smooth_preds": smoothed_preds,
            "file_pred": file_maj_vote_pred
        })

    def calc_percentiles(vals):
        return {
            "min": float(np.min(vals)),
            "p25": float(np.percentile(vals, 25)),
            "median": float(np.median(vals)),
            "p75": float(np.percentile(vals, 75)),
            "max": float(np.max(vals))
        }

    print("\n" + "="*80)
    print("1. WINDOW-LEVEL ACCURACY & PER-FILE AGREEMENT DISTRIBUTION")
    print("="*80)
    print(f"  Music Window Accuracy:  {m_win_correct/m_win_total*100:6.2f}% ({m_win_correct}/{m_win_total} windows)")
    print(f"  Speech Window Accuracy: {s_win_correct/s_win_total*100:6.2f}% ({s_win_correct}/{s_win_total} windows)")
    print(f"  Unweighted Window Mean: {0.5*(m_win_correct/m_win_total + s_win_correct/s_win_total)*100:6.2f}%")

    print("\n  DISTRIBUTION OF PER-FILE AGREEMENT FRACTION (RIGHT ANSWERS / TOTAL WINDOWS IN FILE):")
    pm = calc_percentiles(per_file_agreements_m)
    ps = calc_percentiles(per_file_agreements_s)
    print(f"    MUSIC  (N=125 files): Min={pm['min']*100:5.1f}% | p25={pm['p25']*100:5.1f}% | Median={pm['median']*100:5.1f}% | p75={pm['p75']*100:5.1f}% | Max={pm['max']*100:5.1f}%")
    print(f"    SPEECH (N= 45 files): Min={ps['min']*100:5.1f}% | p25={ps['p25']*100:5.1f}% | Median={ps['median']*100:5.1f}% | p75={ps['p75']*100:5.1f}% | Max={ps['max']*100:5.1f}%")

    print("\n" + "="*80)
    print("2. PER-WINDOW FEATURE SPREAD WITHIN FILES: MEDIAN OF (p75 - p25)")
    print("="*80)
    print(f"  CV Feature Intra-File Interquartile Spread:   Median(p75 - p25) = {np.median(file_window_spreads_cv):.4f}")
    print(f"  Flux Feature Intra-File Interquartile Spread: Median(p75 - p25) = {np.median(file_window_spreads_fl):.4f}")

    print("\n" + "="*80)
    print("3. MAJORITY VOTE OVER 5-WINDOW SPAN (SCOUT SMOOTHING)")
    print("="*80)
    sm_m_win_corr = sum(sum(1 for p in x["smooth_preds"] if p == "music") for x in smoothed_results if x["label"] == "music")
    sm_m_win_tot = sum(len(x["smooth_preds"]) for x in smoothed_results if x["label"] == "music")

    sm_s_win_corr = sum(sum(1 for p in x["smooth_preds"] if p == "speech") for x in smoothed_results if x["label"] == "speech")
    sm_s_win_tot = sum(len(x["smooth_preds"]) for x in smoothed_results if x["label"] == "speech")

    print(f"  Smoothed Music Window Accuracy:  {sm_m_win_corr/sm_m_win_tot*100:6.2f}% ({sm_m_win_corr}/{sm_m_win_tot} windows)")
    print(f"  Smoothed Speech Window Accuracy: {sm_s_win_corr/sm_s_win_tot*100:6.2f}% ({sm_s_win_corr}/{sm_s_win_tot} windows)")
    print(f"  Smoothed Unweighted Window Mean: {0.5*(sm_m_win_corr/sm_m_win_tot + sm_s_win_corr/sm_s_win_tot)*100:6.2f}%")

    file_m_corr = sum(1 for x in smoothed_results if x["label"] == "music" and x["file_pred"] == "music")
    file_m_tot = sum(1 for x in smoothed_results if x["label"] == "music")
    file_s_corr = sum(1 for x in smoothed_results if x["label"] == "speech" and x["file_pred"] == "speech")
    file_s_tot = sum(1 for x in smoothed_results if x["label"] == "speech")

    print(f"\n  PER-FILE CLASSIFICATION ACCURACY AFTER 5-WINDOW MAJORITY VOTING:")
    print(f"    Music File Accuracy:  {file_m_corr/file_m_tot*100:6.2f}% ({file_m_corr}/{file_m_tot} files)")
    print(f"    Speech File Accuracy: {file_s_corr/file_s_tot*100:6.2f}% ({file_s_corr}/{file_s_tot} files)")
    print(f"    Unweighted File Mean: {0.5*(file_m_corr/file_m_tot + file_s_corr/file_s_tot)*100:6.2f}%")

    with open("/tmp/window_evaluation_results.json", "w") as f:
        json.dump({
            "m_win_acc": m_win_correct/m_win_total,
            "s_win_acc": s_win_correct/s_win_total,
            "pm": pm, "ps": ps,
            "file_window_spreads_cv": float(np.median(file_window_spreads_cv)),
            "file_window_spreads_fl": float(np.median(file_window_spreads_fl)),
            "sm_m_win_acc": sm_m_win_corr/sm_m_win_tot,
            "sm_s_win_acc": sm_s_win_corr/sm_s_win_tot,
            "file_m_acc": file_m_corr/file_m_tot,
            "file_s_acc": file_s_corr/file_s_tot,
            "centroids": {
                "mu_m_cv": mu_m_cv, "mu_s_cv": mu_s_cv, "std_cv": std_cv,
                "mu_m_fl": mu_m_fl, "mu_s_fl": mu_s_fl, "std_fl": std_fl
            }
        }, f, indent=2)

if __name__ == "__main__":
    main()
