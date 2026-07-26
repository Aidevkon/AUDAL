#!/usr/bin/env python3
import os
import sys
import json
import numpy as np
import wave
import subprocess
from concurrent.futures import ThreadPoolExecutor

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

def extract_features(filepath):
    cmd_var = ["/home/aidevcon/Documents/creator-os/target/release/measure_variance", filepath, "0.20"]
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

    cmd_mfcc = ["/home/aidevcon/Documents/creator-os/target/release/measure_mfcc", filepath]
    res_mfcc = subprocess.check_output(cmd_mfcc, text=True)
    mfcc_data = json.loads(res_mfcc.strip())
    flux_vals = [w["cepstral_flux"] for w in mfcc_data.get("windows", [])]
    flux = float(np.mean(flux_vals)) if len(flux_vals) > 0 else 0.0

    depth = compute_10ms_frame_metrics(filepath)
    if depth is None:
        raise ValueError(f"Could not compute depth for {filepath}")

    return {"path": filepath, "cv": cv, "flux": flux, "depth": depth}

def main():
    gate_script = "/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py"
    
    res_A = subprocess.run([gate_script, "/tmp/diverse_corpus", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_A_music = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "music" in line]
    passing_A_speech = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "speech" in line]

    source_map = {}
    for i in range(0, 6): source_map[f"music_{i}.wav"] = "LaptopSymphony90"
    for i in range(6, 12): source_map[f"music_{i}.wav"] = "AllSongsEDMRemix"
    for i in range(12, 18): source_map[f"music_{i}.wav"] = "DWK123"
    source_map["music_18.wav"] = "333OfCourseThePersonalityIsGone"
    for i in range(19, 24): source_map[f"music_{i}.wav"] = "AmbientSoundbathPodcast"
    for i in range(24, 30): source_map[f"music_{i}.wav"] = "pkmn-xy-soundtrack"

    for i in range(3, 12): source_map[f"speech_{i}.wav"] = "language_learning_002_librivox"
    source_map["speech_12.wav"] = "170511OSPODCASTJFKcentenary"
    source_map["speech_13.wav"] = "saunders-podcast"
    for i in range(14, 19): source_map[f"speech_{i}.wav"] = "ConversationsFromThePaleBlueDotepisodes078AndOnward"
    for i in range(19, 25): source_map[f"speech_{i}.wav"] = "al-quran-with-kashmiri-koshur-translation-audio-mp3-hq"
    for i in range(25, 30): source_map[f"speech_{i}.wav"] = "art_of_war_librivox"

    train_paths = []
    for fn in passing_A_music: train_paths.append((f"/tmp/diverse_corpus/music/{fn}", fn, "music", source_map[fn]))
    for fn in passing_A_speech: train_paths.append((f"/tmp/diverse_corpus/speech/{fn}", fn, "speech", source_map[fn]))

    print(f"Parallel extracting 40 training files...")
    with ThreadPoolExecutor(max_workers=16) as pool:
        futures = {pool.submit(extract_features, item[0]): item for item in train_paths}
        train_data = []
        for fut in futures:
            item = futures[fut]
            res = fut.result()
            train_data.append({"file": item[1], "label": item[2], "source": item[3], **res})

    m_feats = [x for x in train_data if x["label"] == "music"]
    s_feats = [x for x in train_data if x["label"] == "speech"]

    mu_m_cv = np.mean([x["cv"] for x in m_feats])
    mu_s_cv = np.mean([x["cv"] for x in s_feats])
    std_cv = np.sqrt(0.5 * (np.var([x["cv"] for x in m_feats], ddof=1) + np.var([x["cv"] for x in s_feats], ddof=1)))

    mu_m_fl = np.mean([x["flux"] for x in m_feats])
    mu_s_fl = np.mean([x["flux"] for x in s_feats])
    std_fl = np.sqrt(0.5 * (np.var([x["flux"] for x in m_feats], ddof=1) + np.var([x["flux"] for x in s_feats], ddof=1)))

    mu_m_dp = np.mean([x["depth"] for x in m_feats])
    mu_s_dp = np.mean([x["depth"] for x in s_feats])
    std_dp = np.sqrt(0.5 * (np.var([x["depth"] for x in m_feats], ddof=1) + np.var([x["depth"] for x in s_feats], ddof=1)))

    print("\nTRAINING CENTROIDS & POOLED STDS (FROM 40 TRAINING FILES ONLY):")
    print(f"  CV:    Music Mean = {mu_m_cv:.4f}, Speech Mean = {mu_s_cv:.4f}, Pooled Std = {std_cv:.4f}")
    print(f"  Flux:  Music Mean = {mu_m_fl:.4f}, Speech Mean = {mu_s_fl:.4f}, Pooled Std = {std_fl:.4f}")
    print(f"  DEPTH: Music Mean = {mu_m_dp:.4f}, Speech Mean = {mu_s_dp:.4f}, Pooled Std = {std_dp:.4f}")

    def classify_item(feats, model_type="depth_flux"):
        z_cv = feats["cv"] / std_cv
        z_fl = feats["flux"] / std_fl
        z_dp = feats["depth"] / std_dp

        if model_type == "cv_flux":
            vec = np.array([z_cv, z_fl])
            c_m = np.array([mu_m_cv / std_cv, mu_m_fl / std_fl])
            c_s = np.array([mu_s_cv / std_cv, mu_s_fl / std_fl])
        elif model_type == "depth_flux":
            vec = np.array([z_dp, z_fl])
            c_m = np.array([mu_m_dp / std_dp, mu_m_fl / std_fl])
            c_s = np.array([mu_s_dp / std_dp, mu_s_fl / std_fl])
        elif model_type == "3d":
            vec = np.array([z_dp, z_cv, z_fl])
            c_m = np.array([mu_m_dp / std_dp, mu_m_cv / std_cv, mu_m_fl / std_fl])
            c_s = np.array([mu_s_dp / std_dp, mu_s_cv / std_cv, mu_s_fl / std_fl])

        d_m = np.linalg.norm(vec - c_m)
        d_s = np.linalg.norm(vec - c_s)
        pred = "speech" if d_s < d_m else "music"
        return pred, d_m, d_s

    # Classical (9 files)
    res_B = subprocess.run([gate_script, "/tmp/loose_music", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_classical = [line.split("|")[0].replace("PASS:", "").strip() for line in res_B.stdout.split("\n") if line.startswith("PASS:")]
    class_paths = [f"/tmp/loose_music/music/{fn}" for fn in passing_classical]

    print(f"Parallel extracting 9 classical files...")
    with ThreadPoolExecutor(max_workers=9) as pool:
        classical_feats = list(pool.map(extract_features, class_paths))
    classical_data = [{"file": os.path.basename(res["path"]), "target": "music", **res} for res in classical_feats]

    # User Music (95 files)
    genres = ['techno', 'acoustic', 'metal', 'pop']
    user_music_data = {}
    user_paths = []
    for g in genres:
        g_dir = f"/tmp/user_music/{g}"
        res_g = subprocess.run([gate_script, g_dir, "--allow-mono"], stdout=subprocess.PIPE, text=True)
        passing_g = [line.split("|")[0].replace("PASS:", "").strip() for line in res_g.stdout.split("\n") if line.startswith("PASS:")]
        for fn in passing_g:
            user_paths.append((f"{g_dir}/music/{fn}", g, fn))

    print(f"Parallel extracting 95 user music files...")
    with ThreadPoolExecutor(max_workers=16) as pool:
        u_futures = {pool.submit(extract_features, p[0]): p for p in user_paths}
        for fut in u_futures:
            p_info = u_futures[fut]
            res = fut.result()
            g = p_info[1]
            if g not in user_music_data:
                user_music_data[g] = []
            user_music_data[g].append({"file": p_info[2], "genre": g, "target": "music", **res})

    # Podcasts (26 files)
    with open("/tmp/podcast_results.json", "r") as f:
        pod_json = json.load(f)
    pod_paths = [(item["wav_path"], item["wav_name"], item["show_name"]) for item in pod_json["results"]]

    print(f"Parallel extracting 26 podcast files...")
    with ThreadPoolExecutor(max_workers=16) as pool:
        p_futures = {pool.submit(extract_features, p[0]): p for p in pod_paths}
        podcast_data = []
        for fut in p_futures:
            p_info = p_futures[fut]
            res = fut.result()
            podcast_data.append({"file": p_info[1], "show_name": p_info[2], "target": "speech", **res})

    models = ["cv_flux", "depth_flux", "3d"]
    results_summary = {}

    misclassified_depth_flux = []

    for m_type in models:
        c_preds = [classify_item(x, m_type)[0] for x in classical_data]
        acc_class = sum(1 for p in c_preds if p == "music") / len(classical_data)

        user_accs = {}
        total_user_correct = 0
        total_user_n = 0
        for g in genres:
            g_preds = [classify_item(x, m_type)[0] for x in user_music_data[g]]
            corr = sum(1 for p in g_preds if p == "music")
            user_accs[g] = corr / len(user_music_data[g])
            total_user_correct += corr
            total_user_n += len(user_music_data[g])
        
        acc_user_total = total_user_correct / total_user_n

        p_preds = [classify_item(x, m_type)[0] for x in podcast_data]
        acc_pod = sum(1 for p in p_preds if p == "speech") / len(podcast_data)

        results_summary[m_type] = {
            "classical": acc_class,
            "user_genres": user_accs,
            "user_total": acc_user_total,
            "podcasts": acc_pod
        }

        if m_type == "depth_flux":
            for x, p in zip(classical_data, c_preds):
                if p != "music":
                    _, dm, ds = classify_item(x, m_type)
                    misclassified_depth_flux.append({"set": "Classical", "file": x["file"], "target": "music", "pred": p, "depth": x["depth"], "flux": x["flux"], "d_m": dm, "d_s": ds})
            for g in genres:
                g_preds = [classify_item(x, m_type)[0] for x in user_music_data[g]]
                for x, p in zip(user_music_data[g], g_preds):
                    if p != "music":
                        _, dm, ds = classify_item(x, m_type)
                        misclassified_depth_flux.append({"set": f"User Music ({g.upper()})", "file": x["file"], "target": "music", "pred": p, "depth": x["depth"], "flux": x["flux"], "d_m": dm, "d_s": ds})
            for x, p in zip(podcast_data, p_preds):
                if p != "speech":
                    _, dm, ds = classify_item(x, m_type)
                    misclassified_depth_flux.append({"set": "Podcasts", "file": f"{x['file']} ({x['show_name']})", "target": "speech", "pred": p, "depth": x["depth"], "flux": x["flux"], "d_m": dm, "d_s": ds})

    sources = list(set([x["source"] for x in train_data]))
    print(f"\nEvaluating LOSO over {len(sources)} training sources...")
    
    loso_acc = {}
    for m_type in models:
        loso_correct = 0
        loso_total = 0
        for src in sources:
            train_sub = [x for x in train_data if x["source"] != src]
            test_sub = [x for x in train_data if x["source"] == src]

            m_s = [x for x in train_sub if x["label"] == "music"]
            s_s = [x for x in train_sub if x["label"] == "speech"]

            mu_m_cv_s = np.mean([x["cv"] for x in m_s])
            mu_s_cv_s = np.mean([x["cv"] for x in s_s])
            std_cv_s = np.sqrt(0.5 * (np.var([x["cv"] for x in m_s], ddof=1) + np.var([x["cv"] for x in s_s], ddof=1)))

            mu_m_fl_s = np.mean([x["flux"] for x in m_s])
            mu_s_fl_s = np.mean([x["flux"] for x in s_s])
            std_fl_s = np.sqrt(0.5 * (np.var([x["flux"] for x in m_s], ddof=1) + np.var([x["flux"] for x in s_s], ddof=1)))

            mu_m_dp_s = np.mean([x["depth"] for x in m_s])
            mu_s_dp_s = np.mean([x["depth"] for x in s_s])
            std_dp_s = np.sqrt(0.5 * (np.var([x["depth"] for x in m_s], ddof=1) + np.var([x["depth"] for x in s_s], ddof=1)))

            for t_item in test_sub:
                z_cv = t_item["cv"] / std_cv_s
                z_fl = t_item["flux"] / std_fl_s
                z_dp = t_item["depth"] / std_dp_s

                if m_type == "cv_flux":
                    vec = np.array([z_cv, z_fl])
                    c_m = np.array([mu_m_cv_s / std_cv_s, mu_m_fl_s / std_fl_s])
                    c_s = np.array([mu_s_cv_s / std_cv_s, mu_s_fl_s / std_fl_s])
                elif m_type == "depth_flux":
                    vec = np.array([z_dp, z_fl])
                    c_m = np.array([mu_m_dp_s / std_dp_s, mu_m_fl_s / std_fl_s])
                    c_s = np.array([mu_s_dp_s / std_dp_s, mu_s_fl_s / std_fl_s])
                elif m_type == "3d":
                    vec = np.array([z_dp, z_cv, z_fl])
                    c_m = np.array([mu_m_dp_s / std_dp_s, mu_m_cv_s / std_cv_s, mu_m_fl_s / std_fl_s])
                    c_s = np.array([mu_s_dp_s / std_dp_s, mu_s_cv_s / std_cv_s, mu_s_fl_s / std_fl_s])

                d_m = np.linalg.norm(vec - c_m)
                d_s = np.linalg.norm(vec - c_s)
                pred = "speech" if d_s < d_m else "music"
                if pred == t_item["label"]:
                    loso_correct += 1
                loso_total += 1

        loso_acc[m_type] = loso_correct / loso_total

    print("\n" + "="*95)
    print("COMPARISON TABLE: (CV, FLUX) vs (DEPTH, FLUX) vs (DEPTH, CV, FLUX) 3D")
    print("="*95)
    print(f"{'EVALUATION SET':<28} | {'(CV, FLUX) 2D':<15} | {'(DEPTH, FLUX) 2D':<18} | {'(DEPTH, CV, FLUX) 3D':<20}")
    print("-" * 95)
    print(f"{'12-Source LOSO Training CV':<28} | {loso_acc['cv_flux']*100:6.2f}% ({int(round(loso_acc['cv_flux']*40))}/40)   | {loso_acc['depth_flux']*100:6.2f}% ({int(round(loso_acc['depth_flux']*40))}/40)     | {loso_acc['3d']*100:6.2f}% ({int(round(loso_acc['3d']*40))}/40)")
    print(f"{'9 Classical Music':<28} | {results_summary['cv_flux']['classical']*100:6.2f}% (9/9)     | {results_summary['depth_flux']['classical']*100:6.2f}% ({int(round(results_summary['depth_flux']['classical']*9))}/9)       | {results_summary['3d']['classical']*100:6.2f}% ({int(round(results_summary['3d']['classical']*9))}/9)")
    print(f"{'95 User Music Total':<28} | {results_summary['cv_flux']['user_total']*100:6.2f}% (91/95)   | {results_summary['depth_flux']['user_total']*100:6.2f}% ({int(round(results_summary['depth_flux']['user_total']*95))}/95)     | {results_summary['3d']['user_total']*100:6.2f}% ({int(round(results_summary['3d']['user_total']*95))}/95)")
    print(f"  • Techno (27)             | {results_summary['cv_flux']['user_genres']['techno']*100:6.2f}% (27/27)   | {results_summary['depth_flux']['user_genres']['techno']*100:6.2f}% ({int(round(results_summary['depth_flux']['user_genres']['techno']*27))}/27)     | {results_summary['3d']['user_genres']['techno']*100:6.2f}% ({int(round(results_summary['3d']['user_genres']['techno']*27))}/27)")
    print(f"  • Acoustic (29)           | {results_summary['cv_flux']['user_genres']['acoustic']*100:6.2f}% (27/29)   | {results_summary['depth_flux']['user_genres']['acoustic']*100:6.2f}% ({int(round(results_summary['depth_flux']['user_genres']['acoustic']*29))}/29)     | {results_summary['3d']['user_genres']['acoustic']*100:6.2f}% ({int(round(results_summary['3d']['user_genres']['acoustic']*29))}/29)")
    print(f"  • Metal (15)              | {results_summary['cv_flux']['user_genres']['metal']*100:6.2f}% (15/15)   | {results_summary['depth_flux']['user_genres']['metal']*100:6.2f}% ({int(round(results_summary['depth_flux']['user_genres']['metal']*15))}/15)     | {results_summary['3d']['user_genres']['metal']*100:6.2f}% ({int(round(results_summary['3d']['user_genres']['metal']*15))}/15)")
    print(f"  • Pop (24)                | {results_summary['cv_flux']['user_genres']['pop']*100:6.2f}% (22/24)   | {results_summary['depth_flux']['user_genres']['pop']*100:6.2f}% ({int(round(results_summary['depth_flux']['user_genres']['pop']*24))}/24)     | {results_summary['3d']['user_genres']['pop']*100:6.2f}% ({int(round(results_summary['3d']['user_genres']['pop']*24))}/24)")
    print(f"{'26 Held-Out Podcasts':<28} | {results_summary['cv_flux']['podcasts']*100:6.2f}% (18/26)   | {results_summary['depth_flux']['podcasts']*100:6.2f}% ({int(round(results_summary['depth_flux']['podcasts']*26))}/26)     | {results_summary['3d']['podcasts']*100:6.2f}% ({int(round(results_summary['3d']['podcasts']*26))}/26)")

    print("\n" + "="*95)
    print("EVERY MISCLASSIFIED FILE UNDER (DEPTH, FLUX) 2D MODEL:")
    print("="*95)
    print(f"{'SET':<20} | {'FILE':<40} | {'TARGET':<8} | {'PRED':<8} | {'DEPTH':<8} | {'FLUX':<8} | {'d_m':<6} | {'d_s':<6}")
    print("-" * 115)
    for x in sorted(misclassified_depth_flux, key=lambda k: (k["set"], k["file"])):
        print(f"{x['set']:<20} | {x['file']:<40} | {x['target']:<8} | {x['pred']:<8} | {x['depth']:8.2f} | {x['flux']:8.4f} | {x['d_m']:6.2f} | {x['d_s']:6.2f}")

    with open("/tmp/eval_models_results.json", "w") as f:
        json.dump({
            "results_summary": results_summary,
            "loso_acc": loso_acc,
            "misclassified_depth_flux": misclassified_depth_flux,
            "centroids": {
                "mu_m_cv": mu_m_cv, "mu_s_cv": mu_s_cv, "std_cv": std_cv,
                "mu_m_fl": mu_m_fl, "mu_s_fl": mu_s_fl, "std_fl": std_fl,
                "mu_m_dp": mu_m_dp, "mu_s_dp": mu_s_dp, "std_dp": std_dp
            }
        }, f, indent=2)

if __name__ == "__main__":
    main()
