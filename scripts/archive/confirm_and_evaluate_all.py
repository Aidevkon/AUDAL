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
        dataset_130 = list(pool.map(extract_all_features, all_items))

    # PART 1: RE-RUN LOSO TABLE INCLUDING ALL 130 FILES
    sources_130 = list(set([x["source"] for x in dataset_130]))
    models = ["cv_flux", "depth_flux", "3d"]
    
    loso_results_130 = {m: {"music_correct": 0, "music_total": 0, "speech_correct": 0, "speech_total": 0, "per_source": {}, "misclassified": []} for m in models}

    for src in sources_130:
        train_sub = [x for x in dataset_130 if x["source"] != src]
        test_sub = [x for x in dataset_130 if x["source"] == src]

        m_tr = [x for x in train_sub if x["label"] == "music"]
        s_tr = [x for x in train_sub if x["label"] == "speech"]

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
                    loso_results_130[m_type]["music_total"] += 1
                    if pred == "music":
                        loso_results_130[m_type]["music_correct"] += 1
                    else:
                        loso_results_130[m_type]["misclassified"].append({**t_item, "pred": pred, "d_m": d_m, "d_s": d_s})
                else:
                    loso_results_130[m_type]["speech_total"] += 1
                    if pred == "speech":
                        loso_results_130[m_type]["speech_correct"] += 1
                    else:
                        loso_results_130[m_type]["misclassified"].append({**t_item, "pred": pred, "d_m": d_m, "d_s": d_s})

    print("\n" + "="*80)
    print("CONFIRMATION 1: LOSO ACCURACY ON ALL 130 FILES (INCLUDING SPEECH 16 & 25)")
    print("="*80)
    print(f"{'MODEL':<22} | {'MUSIC ACCURACY (104)':<22} | {'SPEECH ACCURACY (26)':<22} | {'UNWEIGHTED MEAN':<20}")
    print("-" * 90)
    for m_type in models:
        m_corr = loso_results_130[m_type]["music_correct"]
        m_tot = loso_results_130[m_type]["music_total"]
        m_acc = m_corr / m_tot if m_tot > 0 else 0.0

        s_corr = loso_results_130[m_type]["speech_correct"]
        s_tot = loso_results_130[m_type]["speech_total"]
        s_acc = s_corr / s_tot if s_tot > 0 else 0.0

        mean_acc = 0.5 * (m_acc + s_acc)
        name_map = {"cv_flux": "(CV, FLUX) 2D", "depth_flux": "(DEPTH, FLUX) 2D", "3d": "(DEPTH, CV, FLUX) 3D"}
        print(f"{name_map[m_type]:<22} | {m_acc*100:6.2f}% ({m_corr:3d}/{m_tot:3d})       | {s_acc*100:6.2f}% ({s_corr:2d}/{s_tot:2d})       | {mean_acc*100:6.2f}%")

    # PART 2: CLASSIFY THE 40 OLD CORPUS FILES AS A CLEAN HELD-OUT SET
    res_A = subprocess.run([gate_script, "/tmp/diverse_corpus", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_A_music = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "music" in line]
    passing_A_speech = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "speech" in line]

    old_items = []
    for fn in passing_A_music: old_items.append((f"/tmp/diverse_corpus/music/{fn}", "music", "old_music", fn))
    for fn in passing_A_speech: old_items.append((f"/tmp/diverse_corpus/speech/{fn}", "speech", "old_speech", fn))

    print(f"\nExtracting features for 40 old corpus files...")
    with ThreadPoolExecutor(max_workers=16) as pool:
        old_dataset = list(pool.map(extract_all_features, old_items))

    # Compute centroids on all 130 training files
    m_tr_all = [x for x in dataset_130 if x["label"] == "music"]
    s_tr_all = [x for x in dataset_130 if x["label"] == "speech"]

    mu_m_cv_all = np.mean([x["cv"] for x in m_tr_all])
    mu_s_cv_all = np.mean([x["cv"] for x in s_tr_all])
    std_cv_all = np.sqrt(0.5 * (np.var([x["cv"] for x in m_tr_all], ddof=1) + np.var([x["cv"] for x in s_tr_all], ddof=1)))

    mu_m_fl_all = np.mean([x["flux"] for x in m_tr_all])
    mu_s_fl_all = np.mean([x["flux"] for x in s_tr_all])
    std_fl_all = np.sqrt(0.5 * (np.var([x["flux"] for x in m_tr_all], ddof=1) + np.var([x["flux"] for x in s_tr_all], ddof=1)))

    mu_m_dp_all = np.mean([x["depth"] for x in m_tr_all])
    mu_s_dp_all = np.mean([x["depth"] for x in s_tr_all])
    std_dp_all = np.sqrt(0.5 * (np.var([x["depth"] for x in m_tr_all], ddof=1) + np.var([x["depth"] for x in s_tr_all], ddof=1)))

    print("\n" + "="*80)
    print("LOCKED CONFIGURATION: CENTROIDS & POOLED STDS (TRAINED ON ALL 130 FILES)")
    print("="*80)
    print(f"  CV:    Music Mean = {mu_m_cv_all:.4f}, Speech Mean = {mu_s_cv_all:.4f}, Pooled Std = {std_cv_all:.4f}")
    print(f"  Flux:  Music Mean = {mu_m_fl_all:.4f}, Speech Mean = {mu_s_fl_all:.4f}, Pooled Std = {std_fl_all:.4f}")
    print(f"  DEPTH: Music Mean = {mu_m_dp_all:.4f}, Speech Mean = {mu_s_dp_all:.4f}, Pooled Std = {std_dp_all:.4f}")

    old_results = {}
    for m_type in models:
        m_corr = 0
        m_tot = 0
        s_corr = 0
        s_tot = 0
        misc = []

        for x in old_dataset:
            z_cv = x["cv"] / std_cv_all
            z_fl = x["flux"] / std_fl_all
            z_dp = x["depth"] / std_dp_all

            if m_type == "cv_flux":
                vec = np.array([z_cv, z_fl])
                c_m = np.array([mu_m_cv_all / std_cv_all, mu_m_fl_all / std_fl_all])
                c_s = np.array([mu_s_cv_all / std_cv_all, mu_s_fl_all / std_fl_all])
            elif m_type == "depth_flux":
                vec = np.array([z_dp, z_fl])
                c_m = np.array([mu_m_dp_all / std_dp_all, mu_m_fl_all / std_fl_all])
                c_s = np.array([mu_s_dp_all / std_dp_all, mu_s_fl_all / std_fl_all])
            elif m_type == "3d":
                vec = np.array([z_dp, z_cv, z_fl])
                c_m = np.array([mu_m_dp_all / std_dp_all, mu_m_cv_all / std_cv_all, mu_m_fl_all / std_fl_all])
                c_s = np.array([mu_s_dp_all / std_dp_all, mu_s_cv_all / std_cv_all, mu_s_fl_all / std_fl_all])

            d_m = float(np.linalg.norm(vec - c_m))
            d_s = float(np.linalg.norm(vec - c_s))
            pred = "speech" if d_s < d_m else "music"

            if x["label"] == "music":
                m_tot += 1
                if pred == "music": m_corr += 1
                else: misc.append({**x, "pred": pred, "d_m": d_m, "d_s": d_s})
            else:
                s_tot += 1
                if pred == "speech": s_corr += 1
                else: misc.append({**x, "pred": pred, "d_m": d_m, "d_s": d_s})

        old_results[m_type] = {"music_acc": m_corr/m_tot, "speech_acc": s_corr/s_tot, "mean_acc": 0.5*(m_corr/m_tot + s_corr/s_tot), "m_corr": m_corr, "m_tot": m_tot, "s_corr": s_corr, "s_tot": s_tot, "misc": misc}

    print("\n" + "="*80)
    print("CONFIRMATION 2: ACCURACY ON OLD 40-FILE CLEAN HELD-OUT CORPUS")
    print("="*80)
    print(f"{'MODEL':<22} | {'MUSIC ACCURACY (21)':<22} | {'SPEECH ACCURACY (19)':<22} | {'UNWEIGHTED MEAN':<20}")
    print("-" * 90)
    for m_type in models:
        res = old_results[m_type]
        name_map = {"cv_flux": "(CV, FLUX) 2D", "depth_flux": "(DEPTH, FLUX) 2D", "3d": "(DEPTH, CV, FLUX) 3D"}
        print(f"{name_map[m_type]:<22} | {res['music_acc']*100:6.2f}% ({res['m_corr']:2d}/{res['m_tot']:2d})       | {res['speech_acc']*100:6.2f}% ({res['s_corr']:2d}/{res['s_tot']:2d})       | {res['mean_acc']*100:6.2f}%")

    print("\nMISCLASSIFIED FILES ON OLD 40-FILE HELD-OUT CORPUS [(CV, FLUX) 2D MODEL]:")
    for x in old_results["cv_flux"]["misc"]:
        print(f"  • {x['filename']:<15} | Label: {x['label']:<6} | Pred: {x['pred']:<6} | CV={x['cv']:.4f}, Flux={x['flux']:.4f}, Depth={x['depth']:.2f} dB | d_m={x['d_m']:.2f}, d_s={x['d_s']:.2f}")

    print("\nMISCLASSIFIED FILES ON OLD 40-FILE HELD-OUT CORPUS [(DEPTH, CV, FLUX) 3D MODEL]:")
    for x in old_results["3d"]["misc"]:
        print(f"  • {x['filename']:<15} | Label: {x['label']:<6} | Pred: {x['pred']:<6} | CV={x['cv']:.4f}, Flux={x['flux']:.4f}, Depth={x['depth']:.2f} dB | d_m={x['d_m']:.2f}, d_s={x['d_s']:.2f}")

if __name__ == "__main__":
    main()
