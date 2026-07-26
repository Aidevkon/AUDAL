#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np
import re
import shutil

def get_metadata(fpath):
    cmd = ['ffprobe', '-v', 'quiet', '-print_format', 'json', '-show_format', '-show_streams', fpath]
    try:
        res = subprocess.check_output(cmd, text=True)
        info = json.loads(res)
        fmt = info.get('format', {})
        duration = float(fmt.get('duration', 0.0))
        tags = fmt.get('tags', {})
        artist = tags.get('artist', tags.get('ARTIST', 'Unknown Artist'))
        album = tags.get('album', tags.get('ALBUM', 'Unknown Album'))
        title = tags.get('title', tags.get('TITLE', os.path.basename(fpath)))
        return {"duration": duration, "artist": artist, "album": album, "title": title}
    except Exception as e:
        return {"duration": 0.0, "artist": "Unknown", "album": "Unknown", "title": os.path.basename(fpath)}

def main():
    folders = {
        'techno': '/home/aidevcon/Music/TECHNO',
        'acoustic': '/home/aidevcon/Downloads/ACOUSTICS_SAMPLES',
        'metal': '/home/aidevcon/Music/METAL',
        'pop': '/home/aidevcon/Music/POP'
    }

    tmp_base = "/tmp/user_music"
    if os.path.exists(tmp_base):
        shutil.rmtree(tmp_base)
    os.makedirs(tmp_base, exist_ok=True)

    print("============================================================")
    print("STEP 2: SOURCE DIVERSITY GROUPING & SELECTION")
    print("============================================================")

    selected_files = {}

    for g_name, g_path in folders.items():
        print(f"\n--- GENRE: {g_name.upper()} ({g_path}) ---")
        top_files = [os.path.join(g_path, f) for f in os.listdir(g_path) if os.path.isfile(os.path.join(g_path, f))]
        audio_files = sorted([f for f in top_files if f.lower().endswith(('.mp3', '.flac', '.wav', '.m4a', '.aac', '.ogg', '.opus', '.aiff', '.wma'))])

        # Group by album
        album_groups = {}
        for f in audio_files:
            meta = get_metadata(f)
            alb = meta["album"]
            if alb == "Unknown Album":
                alb = f"Single_{meta['artist']}"
            if alb not in album_groups:
                album_groups[alb] = []
            album_groups[alb].append((f, meta))

        print(f"Total top-level files: {len(audio_files)} across {len(album_groups)} album/groupings.")

        g_selected = []
        for alb, items in album_groups.items():
            chosen = items[:2] # At most 2 files per album
            for item in chosen:
                g_selected.append(item)
            print(f"  Group '{alb}': {len(items)} files -> Selected {len(chosen)}")

        selected_files[g_name] = g_selected
        print(f"Total Selected for {g_name.upper()}: {len(g_selected)} files.")

    print("\n============================================================")
    print("STEP 3: COPY & CONVERT TO /tmp/user_music/<genre>/music/")
    print("============================================================")

    converted_files = {}

    for g_name, items in selected_files.items():
        out_dir = os.path.join(tmp_base, g_name, "music")
        os.makedirs(out_dir, exist_ok=True)
        converted_files[g_name] = []

        for idx, (fpath, meta) in enumerate(items):
            dur = meta["duration"]
            out_filename = f"music_{idx}.wav"
            out_path = os.path.join(out_dir, out_filename)

            if dur < 90.0:
                ss_arg = []
                print(f"  Short file ({dur:.1f}s < 90s): {os.path.basename(fpath)} -> start from 0s")
            else:
                ss_arg = ["-ss", "60"]

            cmd = ["ffmpeg"] + ss_arg + ["-t", "30", "-i", fpath, "-ar", "48000", "-ac", "2", "-c:a", "pcm_s16le", out_path, "-y"]
            subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)

            converted_files[g_name].append({
                "wav_path": out_path, "wav_name": out_filename, "orig_path": fpath, "meta": meta
            })

    print("\n============================================================")
    print("STEP 4: RUNNING GATE ON ALL SUBFOLDERS")
    print("============================================================")

    gate_script = "/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py"
    passed_by_genre = {}

    for g_name in folders.keys():
        g_dir = os.path.join(tmp_base, g_name)
        print(f"\n--- Gating {g_name.upper()} ---")
        res = subprocess.run([gate_script, g_dir, "--allow-mono"], stdout=subprocess.PIPE, text=True)

        passed = []
        for line in res.stdout.split("\n"):
            if line.startswith("PASS:") or line.startswith("REJECT:"):
                print(f"  {line}")
                if line.startswith("PASS:"):
                    fn = line.split("|")[0].replace("PASS:", "").strip()
                    passed.append(fn)

        passed_by_genre[g_name] = passed

    print("\n============================================================")
    print("STEP 5 & 6: MEASURING METRICS AND CLASSIFYING WITH EXISTING CENTROIDS")
    print("============================================================")

    # 1. First, calculate the existing 40-file corpus centroids
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
                file_to_source[f"{cat}_{idx}.wav"] = ident
                class_counts[cat] += 1

    res_A = subprocess.run([gate_script, "/tmp/diverse_corpus", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_A = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:")]

    bin_var = "/home/aidevcon/Documents/creator-os/target/release/measure_variance"
    bin_mfcc = "/home/aidevcon/Documents/creator-os/target/release/measure_mfcc"

    train_data = []
    for fn in passing_A:
        cat = "music" if fn.startswith("music") else "speech"
        path = f"/tmp/diverse_corpus/{cat}/{fn}"
        
        # CV
        out_v = subprocess.check_output([bin_var, path, "0.20"], text=True).strip()
        cvs = [json.loads(l)["cv"] for l in out_v.split("\n") if l.strip()]
        cv_val = np.median(cvs)

        # Flux
        out_m = subprocess.check_output([bin_mfcc, path], text=True).strip()
        flux_val = float(json.loads(out_m)["flux_median"])

        train_data.append({"file": fn, "class": cat, "cv": cv_val, "flux": flux_val})

    tr_m_cv = [x["cv"] for x in train_data if x["class"] == "music"]
    tr_m_fl = [x["flux"] for x in train_data if x["class"] == "music"]
    tr_s_cv = [x["cv"] for x in train_data if x["class"] == "speech"]
    tr_s_fl = [x["flux"] for x in train_data if x["class"] == "speech"]

    m_m_cv, m_m_fl = np.mean(tr_m_cv), np.mean(tr_m_fl)
    m_s_cv, m_s_fl = np.mean(tr_s_cv), np.mean(tr_s_fl)

    std_cv = np.sqrt(((len(tr_m_cv)-1)*np.var(tr_m_cv, ddof=1) + (len(tr_s_cv)-1)*np.var(tr_s_cv, ddof=1)) / (len(train_data)-2))
    std_fl = np.sqrt(((len(tr_m_fl)-1)*np.var(tr_m_fl, ddof=1) + (len(tr_s_fl)-1)*np.var(tr_s_fl, ddof=1)) / (len(train_data)-2))

    c_m_z = np.array([m_m_cv / std_cv, m_m_fl / std_fl])
    c_s_z = np.array([m_s_cv / std_cv, m_s_fl / std_fl])

    print(f"TRAINED CENTROIDS (Z-SCORED FROM EXISTING 40-FILE CORPUS):")
    print(f"  Music Centroid:  CV_mean={m_m_cv:.4f}, Flux_mean={m_m_fl:.4f} -> Z: {c_m_z}")
    print(f"  Speech Centroid: CV_mean={m_s_cv:.4f}, Flux_mean={m_s_fl:.4f} -> Z: {c_s_z}")
    print(f"  Pooled Stds:     std_CV={std_cv:.4f}, std_Flux={std_fl:.4f}")

    # Now evaluate user files
    results_by_genre = {}

    for g_name in folders.keys():
        print(f"\n============================================================")
        print(f"EVALUATING GENRE: {g_name.upper()}")
        print(f"============================================================")

        passed_files = passed_by_genre[g_name]
        g_results = []

        for fn in passed_files:
            wav_path = os.path.join(tmp_base, g_name, "music", fn)

            # Measure CV
            out_v = subprocess.check_output([bin_var, wav_path, "0.20"], text=True).strip()
            cvs = [json.loads(l)["cv"] for l in out_v.split("\n") if l.strip()]
            cv_val = np.median(cvs)

            # Measure Flux
            out_m = subprocess.check_output([bin_mfcc, wav_path], text=True).strip()
            flux_val = float(json.loads(out_m)["flux_median"])

            # Z-score against trained stds
            z_item = np.array([cv_val / std_cv, flux_val / std_fl])
            d_m = np.linalg.norm(z_item - c_m_z)
            d_s = np.linalg.norm(z_item - c_s_z)

            pred = "music" if d_m <= d_s else "speech"

            # Match metadata
            meta_info = next((item["meta"] for item in converted_files[g_name] if item["wav_name"] == fn), {})

            g_results.append({
                "file": fn, "orig": meta_info.get("title", fn), "artist": meta_info.get("artist", ""),
                "cv": cv_val, "flux": flux_val, "pred": pred, "d_m": d_m, "d_s": d_s
            })

        results_by_genre[g_name] = g_results

        # Summary for genre
        n_total = len(g_results)
        n_music = sum(1 for x in g_results if x["pred"] == "music")
        n_speech = sum(1 for x in g_results if x["pred"] == "speech")

        cvs_g = [x["cv"] for x in g_results]
        fls_g = [x["flux"] for x in g_results]

        print(f"CLASSIFICATION SUMMARY FOR {g_name.upper()}:")
        print(f"  Total Passed Files: {n_total}")
        if n_total > 0:
            print(f"  Classified MUSIC:   {n_music} / {n_total} ({n_music/n_total*100:.1f}%)")
            print(f"  Classified SPEECH:  {n_speech} / {n_total} ({n_speech/n_total*100:.1f}%)")

            print(f"2D COORDINATES FOR {g_name.upper()}:")
            print(f"  CV:   Min={np.min(cvs_g):.4f}, Median={np.median(cvs_g):.4f}, Max={np.max(cvs_g):.4f}")
            print(f"  Flux: Min={np.min(fls_g):.4f}, Median={np.median(fls_g):.4f}, Max={np.max(fls_g):.4f}")

        if n_speech > 0:
            print(f"\n  FAILURES IN {g_name.upper()} (CLASSIFIED AS SPEECH):")
            for x in g_results:
                if x["pred"] == "speech":
                    print(f"    • {x['file']} ({x['artist']} - {x['orig']}) | CV={x['cv']:.4f}, Flux={x['flux']:.4f} (d_m={x['d_m']:.2f}, d_s={x['d_s']:.2f})")

    # OVERALL SUMMARY
    total_files_all = sum(len(r) for r in results_by_genre.values())
    total_music_all = sum(sum(1 for x in r if x["pred"] == "music") for r in results_by_genre.values())

    print("\n============================================================")
    print("FINAL HELD-OUT USER MUSIC TEST VERDICT")
    print("============================================================")
    if total_files_all > 0:
        print(f"OVERALL ACCURACY ACROSS ALL USER MUSIC: {total_music_all} / {total_files_all} ({total_music_all/total_files_all*100:.2f}%) classified as MUSIC")

if __name__ == "__main__":
    main()
