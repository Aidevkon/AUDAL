#!/usr/bin/env python3
import os
import sys
import json
import numpy as np
import wave
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
        
        if artist and album:
            src = f"{artist} - {album}"
        elif artist:
            src = artist
        else:
            src = os.path.basename(fpath).split('.')[0]
        return {"artist": artist, "album": album, "title": title, "source": src}
    except Exception:
        return {"artist": "", "album": "", "title": os.path.basename(fpath), "source": os.path.basename(fpath).split('.')[0]}

def compute_10ms_frame_metrics(filepath):
    try:
        with wave.open(filepath, 'rb') as wf:
            n_channels = wf.getnchannels()
            sample_width = wf.getsampwidth()
            framerate = wf.getframerate()
            n_frames = wf.getnframes()
            raw_bytes = wf.readframes(n_frames)
        
        if sample_width == 2:
            data = np.frombuffer(raw_bytes, dtype=np.int16).astype(np.float64) / 32768.0
        elif sample_width == 4:
            data = np.frombuffer(raw_bytes, dtype=np.int32).astype(np.float64) / 2147483648.0
        else:
            return None

        if n_channels > 1:
            data = data.reshape(-1, n_channels).mean(axis=1)

        frame_len = int(framerate * 0.010)
        num_frames = len(data) // frame_len
        if num_frames == 0:
            return None

        frames = data[:num_frames * frame_len].reshape(num_frames, frame_len)
        rms = np.sqrt(np.mean(frames**2, axis=1))
        eps = 1e-12
        rms_db = 20.0 * np.log10(rms + eps)

        median_db = float(np.median(rms_db))
        p5_db = float(np.percentile(rms_db, 5))
        depth = median_db - p5_db

        return depth
    except Exception:
        return None

def extract_all_features(item):
    path, label, group, source_name = item
    
    # 1. CV at FLUX_THRESHOLD 0.20
    cmd_var = ["/home/aidevcon/Documents/creator-os/target/release/measure_variance", path, "0.20"]
    res_var = subprocess.check_output(cmd_var, text=True)
    cv_vals = []
    for line in res_var.strip().split("\n"):
        if line.strip():
            try:
                data = json.loads(line)
                if "cv" in data:
                    cv_vals.append(data["cv"])
            except Exception:
                pass
    cv = float(np.mean(cv_vals)) if len(cv_vals) > 0 else 0.0

    # 2. Cepstral Flux
    cmd_mfcc = ["/home/aidevcon/Documents/creator-os/target/release/measure_mfcc", path]
    res_mfcc = subprocess.check_output(cmd_mfcc, text=True)
    mfcc_data = json.loads(res_mfcc.strip())
    flux_vals = [w["cepstral_flux"] for w in mfcc_data.get("windows", [])]
    flux = float(np.mean(flux_vals)) if len(flux_vals) > 0 else 0.0

    # 3. DEPTH
    depth = compute_10ms_frame_metrics(path)

    return {
        "path": path,
        "filename": os.path.basename(path),
        "label": label,
        "group": group,
        "source": source_name,
        "cv": cv,
        "flux": flux,
        "depth": depth
    }

def main():
    gate_script = "/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py"
    all_items = []

    # 1. User Music (95 files)
    genres = ['techno', 'acoustic', 'metal', 'pop']
    for g in genres:
        g_dir = f"/tmp/user_music/{g}"
        res_g = subprocess.run([gate_script, g_dir, "--allow-mono"], stdout=subprocess.PIPE, text=True)
        passing_g = [line.split("|")[0].replace("PASS:", "").strip() for line in res_g.stdout.split("\n") if line.startswith("PASS:")]
        for fn in passing_g:
            p = f"{g_dir}/music/{fn}"
            meta = get_metadata(p)
            src = f"user_music_{g}_{meta['source']}"
            all_items.append((p, "music", f"user_music_{g}", src))

    # 2. Classical Music (9 files)
    res_B = subprocess.run([gate_script, "/tmp/loose_music", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_classical = [line.split("|")[0].replace("PASS:", "").strip() for line in res_B.stdout.split("\n") if line.startswith("PASS:")]
    for fn in passing_classical:
        p = f"/tmp/loose_music/music/{fn}"
        src = f"classical_{fn.split('.')[0]}"
        all_items.append((p, "music", "classical", src))

    # 3. Podcasts (26 files)
    with open("/tmp/podcast_results.json", "r") as f:
        pod_json = json.load(f)
    for item in pod_json["results"]:
        p = item["wav_path"]
        src = f"podcast_{item['show_name']}"
        all_items.append((p, "speech", "podcast", src))

    print(f"Extracting features for all {len(all_items)} consistently-extracted files...")
    with ThreadPoolExecutor(max_workers=16) as pool:
        dataset = list(pool.map(extract_all_features, all_items))

    # SANITY & ARTIFACT EXCLUSION (>60 dB DEPTH)
    music_items = [x for x in dataset if x["label"] == "music"]
    speech_items = [x for x in dataset if x["label"] == "speech"]

    excluded_files = [x for x in dataset if x["depth"] > 60.0]
    valid_dataset = [x for x in dataset if x["depth"] <= 60.0]
    valid_music = [x for x in valid_dataset if x["label"] == "music"]
    valid_speech = [x for x in valid_dataset if x["label"] == "speech"]

    def calc_percentiles(vals):
        return {
            "min": float(np.min(vals)),
            "p25": float(np.percentile(vals, 25)),
            "median": float(np.median(vals)),
            "p75": float(np.percentile(vals, 75)),
            "max": float(np.max(vals))
        }

    print("\n" + "="*80)
    print("SANITY DISTRIBUTION REPORT (PER CLASS STATISTICS)")
    print("="*80)
    
    for label, group in [("MUSIC (N = 104)", valid_music), ("SPEECH (N = 26)", valid_speech)]:
        print(f"\n--- {label} ---")
        for feat_name in ["depth", "cv", "flux"]:
            vals = [x[feat_name] for x in group]
            p = calc_percentiles(vals)
            print(f"  {feat_name.upper():<6} | Min: {p['min']:7.4f} | p25: {p['p25']:7.4f} | Median: {p['median']:7.4f} | p75: {p['p75']:7.4f} | Max: {p['max']:7.4f}")

    print("\n" + "="*80)
    print("EXCLUDED FILES (DEPTH > 60 dB ARTIFACT FILTER)")
    print("="*80)
    if len(excluded_files) == 0:
        print("  NONE! All 130 files have DEPTH <= 60 dB (zero silent-tail artifacts found).")
    else:
        for x in excluded_files:
            print(f"  EXCLUDED: {x['filename']} ({x['source']}) | DEPTH = {x['depth']:.2f} dB (>60dB silent-tail artifact)")

    # LEAVE-ONE-SOURCE-OUT CROSS-VALIDATION
    sources = list(set([x["source"] for x in valid_dataset]))
    models = ["cv_flux", "depth_flux", "3d"]
    
    loso_results = {m: {"music_correct": 0, "music_total": 0, "speech_correct": 0, "speech_total": 0, "per_source": {}, "misclassified": []} for m in models}

    for src in sources:
        # Hold out src
        train_sub = [x for x in valid_dataset if x["source"] != src]
        test_sub = [x for x in valid_dataset if x["source"] == src]

        m_tr = [x for x in train_sub if x["label"] == "music"]
        s_tr = [x for x in train_sub if x["label"] == "speech"]

        # Centroids & Pooled Stds for this fold
        mu_m_cv = np.mean([x["cv"] for x in m_tr])
        mu_s_cv = np.mean([x["cv"] for x in s_tr])
        std_cv = np.sqrt(0.5 * (np.var([x["cv"] for x in m_tr], ddof=1) + np.var([x["cv"] for x in s_tr], ddof=1)))

        mu_m_fl = np.mean([x["flux"] for x in m_tr])
        mu_s_fl = np.mean([x["flux"] for x in s_tr])
        std_fl = np.sqrt(0.5 * (np.var([x["flux"] for x in m_tr], ddof=1) + np.var([x["flux"] for x in s_tr], ddof=1)))

        mu_m_dp = np.mean([x["depth"] for x in m_tr])
        mu_s_dp = np.mean([x["depth"] for x in s_tr])
        std_dp = np.sqrt(0.5 * (np.var([x["depth"] for x in m_tr], ddof=1) + np.var([x["depth"] for x in s_tr], ddof=1)))

        for m_type in models:
            src_correct = 0
            src_total = len(test_sub)

            for t_item in test_sub:
                z_cv = t_item["cv"] / std_cv
                z_fl = t_item["flux"] / std_fl
                z_dp = t_item["depth"] / std_dp

                if m_type == "cv_flux":
                    vec = np.array([z_cv, z_fl])
                    c_m = np.array([mu_m_cv / std_cv, mu_m_fl / std_fl])
                    c_s = np.array([mu_s_cv / std_cv, mu_s_fl / std_fl])
                elif m_type == "depth_flux":
                    vec = np.array([z_dp, z_fl])
                    c_m = np.array([mu_m_dp / std_dp, mu_m_fl / std_fl])
                    c_s = np.array([mu_s_dp / std_dp, mu_s_fl / std_fl])
                elif m_type == "3d":
                    vec = np.array([z_dp, z_cv, z_fl])
                    c_m = np.array([mu_m_dp / std_dp, mu_m_cv / std_cv, mu_m_fl / std_fl])
                    c_s = np.array([mu_s_dp / std_dp, mu_s_cv / std_cv, mu_s_fl / std_fl])

                d_m = float(np.linalg.norm(vec - c_m))
                d_s = float(np.linalg.norm(vec - c_s))
                pred = "speech" if d_s < d_m else "music"

                if t_item["label"] == "music":
                    loso_results[m_type]["music_total"] += 1
                    if pred == "music":
                        loso_results[m_type]["music_correct"] += 1
                        src_correct += 1
                    else:
                        loso_results[m_type]["misclassified"].append({**t_item, "pred": pred, "d_m": d_m, "d_s": d_s})
                else:
                    loso_results[m_type]["speech_total"] += 1
                    if pred == "speech":
                        loso_results[m_type]["speech_correct"] += 1
                        src_correct += 1
                    else:
                        loso_results[m_type]["misclassified"].append({**t_item, "pred": pred, "d_m": d_m, "d_s": d_s})

            loso_results[m_type]["per_source"][src] = (src_correct, src_total)

    # PRINT LOSO TABLE
    print("\n" + "="*80)
    print("LEAVE-ONE-SOURCE-OUT (LOSO) ACCURACY COMPARISON TABLE")
    print("="*80)
    print(f"{'FEATURE SPACE':<22} | {'MUSIC ACCURACY (104)':<22} | {'SPEECH ACCURACY (26)':<22} | {'OVERALL UNWEIGHTED MEAN':<22}")
    print("-" * 90)
    
    best_model = None
    best_mean_acc = -1.0

    for m_type in models:
        m_corr = loso_results[m_type]["music_correct"]
        m_tot = loso_results[m_type]["music_total"]
        m_acc = m_corr / m_tot if m_tot > 0 else 0.0

        s_corr = loso_results[m_type]["speech_correct"]
        s_tot = loso_results[m_type]["speech_total"]
        s_acc = s_corr / s_tot if s_tot > 0 else 0.0

        unweighted_mean = 0.5 * (m_acc + s_acc)
        if unweighted_mean > best_mean_acc:
            best_mean_acc = unweighted_mean
            best_model = m_type

        name_map = {"cv_flux": "(CV, FLUX) 2D", "depth_flux": "(DEPTH, FLUX) 2D", "3d": "(DEPTH, CV, FLUX) 3D"}
        print(f"{name_map[m_type]:<22} | {m_acc*100:6.2f}% ({m_corr:3d}/{m_tot:3d})       | {s_acc*100:6.2f}% ({s_corr:2d}/{s_tot:2d})       | {unweighted_mean*100:6.2f}%")

    print(f"\nWINNING MODEL: {best_model.upper()} (Unweighted Mean Accuracy: {best_mean_acc*100:.2f}%)")

    # WINNING MODEL PER-SOURCE BREAKDOWN & MISCLASSIFICATIONS
    win_res = loso_results[best_model]
    print("\n" + "="*80)
    print(f"WINNING MODEL [{best_model.upper()}] MISCLASSIFIED FILES ({len(win_res['misclassified'])} total)")
    print("="*80)
    print(f"{'FILE':<35} | {'LABEL':<7} | {'PRED':<7} | {'DEPTH':<7} | {'CV':<7} | {'FLUX':<7} | {'d_m':<5} | {'d_s':<5}")
    print("-" * 95)
    for x in win_res['misclassified']:
        print(f"{x['filename']:<35} | {x['label']:<7} | {x['pred']:<7} | {x['depth']:7.2f} | {x['cv']:7.4f} | {x['flux']:7.4f} | {x['d_m']:5.2f} | {x['d_s']:5.2f}")

    with open("/tmp/rebuild_130_results.json", "w") as f:
        json.dump({
            "valid_dataset": valid_dataset,
            "excluded_files": excluded_files,
            "loso_results": loso_results,
            "best_model": best_model
        }, f, indent=2)

if __name__ == "__main__":
    main()
