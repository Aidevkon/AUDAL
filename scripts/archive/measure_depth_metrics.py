#!/usr/bin/env python3
import os
import sys
import json
import numpy as np
import wave
import subprocess

def get_metadata(fpath):
    cmd = ['ffprobe', '-v', 'quiet', '-print_format', 'json', '-show_format', '-show_streams', fpath]
    try:
        res = subprocess.check_output(cmd, text=True)
        info = json.loads(res)
        fmt = info.get('format', {})
        tags = fmt.get('tags', {})
        artist = tags.get('artist', tags.get('ARTIST', 'Unknown Artist'))
        album = tags.get('album', tags.get('ALBUM', 'Unknown Album'))
        title = tags.get('title', tags.get('TITLE', os.path.basename(fpath)))
        return {"artist": artist, "album": album, "title": title}
    except Exception:
        return {"artist": "Unknown", "album": "Unknown", "title": os.path.basename(fpath)}

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

        thresh_25 = median_db - 25.0
        is_quiet = (rms_db < thresh_25)
        silence_fraction = float(np.mean(is_quiet))

        max_run = 0
        current_run = 0
        for q in is_quiet:
            if q:
                current_run += 1
                if current_run > max_run:
                    max_run = current_run
            else:
                current_run = 0
        longest_quiet_run_ms = float(max_run * 10.0)

        return {
            "median_db": median_db,
            "p5_db": p5_db,
            "depth_db": depth,
            "silence_fraction": silence_fraction,
            "longest_quiet_run_ms": longest_quiet_run_ms
        }
    except Exception as e:
        print(f"Error processing {filepath}: {e}", file=sys.stderr)
        return None

def summarize_set(items, label):
    depths = [x["depth_db"] for x in items]
    s_fracs = [x["silence_fraction"] for x in items]
    q_runs = [x["longest_quiet_run_ms"] for x in items]

    print(f"\n============================================================")
    print(f"SET SUMMARY: {label} (N = {len(items)})")
    print(f"============================================================")
    print(f"  DEPTH (dB):            Min = {np.min(depths):5.2f} | Median = {np.median(depths):5.2f} | Max = {np.max(depths):5.2f} | Mean = {np.mean(depths):5.2f}")
    print(f"  SILENCE FRACTION (<25dB): Min = {np.min(s_fracs):.4f} | Median = {np.median(s_fracs):.4f} | Max = {np.max(s_fracs):.4f} | Mean = {np.mean(s_fracs):.4f}")
    print(f"  LONGEST QUIET RUN (ms): Min = {np.min(q_runs):5.1f} | Median = {np.median(q_runs):5.1f} | Max = {np.max(q_runs):5.1f} | Mean = {np.mean(q_runs):5.1f}")

def main():
    with open("/tmp/podcast_results.json", "r") as f:
        pod_data = json.load(f)

    # 1. SET 1: THE 26 PODCASTS (18 Correct vs 8 Errors)
    results_pod_correct = []
    results_pod_error = []

    for item in pod_data["results"]:
        fn = item["wav_name"]
        path = item["wav_path"]
        m = compute_10ms_frame_metrics(path)
        if m:
            entry = {**item, **m}
            if item["pred"] == "speech":
                results_pod_correct.append(entry)
            else:
                results_pod_error.append(entry)

    print("\n" + "="*80)
    print("SET 1: THE 26 PODCASTS (SPLIT INTO 18 CORRECT & 8 ERRORS)")
    print("="*80)
    print(f"\n--- 8 PODCAST ERRORS (CLASSIFIED AS MUSIC) ---")
    print(f"{'FILE':<14} | {'SHOW NAME':<32} | {'MEDIAN dB':<9} | {'P5 dB':<8} | {'DEPTH dB':<8} | {'SIL FRAC':<8} | {'QUIET RUN':<9}")
    print("-" * 105)
    for x in sorted(results_pod_error, key=lambda k: k["depth_db"]):
        print(f"{x['wav_name']:<14} | {x['show_name']:<32} | {x['median_db']:9.2f} | {x['p5_db']:8.2f} | {x['depth_db']:8.2f} | {x['silence_fraction']:8.4f} | {x['longest_quiet_run_ms']:7.0f} ms")

    print(f"\n--- 18 PODCAST CORRECT (CLASSIFIED AS SPEECH) ---")
    print(f"{'FILE':<14} | {'SHOW NAME':<32} | {'MEDIAN dB':<9} | {'P5 dB':<8} | {'DEPTH dB':<8} | {'SIL FRAC':<8} | {'QUIET RUN':<9}")
    print("-" * 105)
    for x in sorted(results_pod_correct, key=lambda k: k["depth_db"]):
        print(f"{x['wav_name']:<14} | {x['show_name']:<32} | {x['median_db']:9.2f} | {x['p5_db']:8.2f} | {x['depth_db']:8.2f} | {x['silence_fraction']:8.4f} | {x['longest_quiet_run_ms']:7.0f} ms")

    summarize_set(results_pod_correct, "1A. PODCASTS CORRECT (SPEECH)")
    summarize_set(results_pod_error, "1B. PODCASTS ERRORS (CLASSIFIED MUSIC)")

    # 2. SET 2: THE 5 TRAINING SPEECH SOURCES
    gate_script = "/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py"
    res_A = subprocess.run([gate_script, "/tmp/diverse_corpus", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_A_speech = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "speech" in line]

    results_train_speech = []
    for fn in passing_A_speech:
        path = f"/tmp/diverse_corpus/speech/{fn}"
        m = compute_10ms_frame_metrics(path)
        if m:
            results_train_speech.append({"file": fn, **m})

    print("\n" + "="*80)
    print("SET 2: THE 5 TRAINING SPEECH SOURCES")
    print("="*80)
    print(f"{'FILE':<35} | {'MEDIAN dB':<9} | {'P5 dB':<8} | {'DEPTH dB':<8} | {'SIL FRAC':<8} | {'QUIET RUN':<9}")
    print("-" * 85)
    for x in sorted(results_train_speech, key=lambda k: k["depth_db"]):
        print(f"{x['file']:<35} | {x['median_db']:9.2f} | {x['p5_db']:8.2f} | {x['depth_db']:8.2f} | {x['silence_fraction']:8.4f} | {x['longest_quiet_run_ms']:7.0f} ms")

    summarize_set(results_train_speech, "2. TRAINING SPEECH SOURCES")

    # 3. SET 3: THE 21 TRAINING MUSIC FILES PLUS THE 9 CLASSICAL
    passing_A_music = [line.split("|")[0].replace("PASS:", "").strip() for line in res_A.stdout.split("\n") if line.startswith("PASS:") and "music" in line]
    res_B = subprocess.run([gate_script, "/tmp/loose_music", "--allow-mono"], stdout=subprocess.PIPE, text=True)
    passing_B_classical = [line.split("|")[0].replace("PASS:", "").strip() for line in res_B.stdout.split("\n") if line.startswith("PASS:")]

    results_train_music = []
    for fn in passing_A_music:
        path = f"/tmp/diverse_corpus/music/{fn}"
        m = compute_10ms_frame_metrics(path)
        if m:
            results_train_music.append({"file": fn, "group": "train_music", **m})

    results_classical = []
    for fn in passing_B_classical:
        path = f"/tmp/loose_music/music/{fn}"
        m = compute_10ms_frame_metrics(path)
        if m:
            results_classical.append({"file": fn, "group": "classical", **m})

    results_set3_all = results_train_music + results_classical

    print("\n" + "="*80)
    print("SET 3: 21 TRAINING MUSIC + 9 CLASSICAL FILES")
    print("="*80)
    print(f"{'FILE':<35} | {'GROUP':<12} | {'MEDIAN dB':<9} | {'P5 dB':<8} | {'DEPTH dB':<8} | {'SIL FRAC':<8} | {'QUIET RUN':<9}")
    print("-" * 100)
    for x in sorted(results_set3_all, key=lambda k: k["depth_db"]):
        print(f"{x['file']:<35} | {x['group']:<12} | {x['median_db']:9.2f} | {x['p5_db']:8.2f} | {x['depth_db']:8.2f} | {x['silence_fraction']:8.4f} | {x['longest_quiet_run_ms']:7.0f} ms")

    summarize_set(results_train_music, "3A. TRAINING MUSIC (21 files)")
    summarize_set(results_classical, "3B. CLASSICAL MUSIC (9 files)")
    summarize_set(results_set3_all, "3C. ALL MUSIC SET 3 (30 files)")

    # 4. SET 4: THE 95 USER MUSIC FILES, GROUPED BY GENRE
    genres = ['techno', 'acoustic', 'metal', 'pop']
    results_user_music = {}

    print("\n" + "="*80)
    print("SET 4: THE 95 USER MUSIC FILES (GROUPED BY GENRE)")
    print("="*80)

    all_user_items = []
    vocal_target_items = []

    target_keywords = ["broussard", "bonny", "billie", "drake", "light horseman", "marc"]

    for g_name in genres:
        g_dir = f"/tmp/user_music/{g_name}"
        res_g = subprocess.run([gate_script, g_dir, "--allow-mono"], stdout=subprocess.PIPE, text=True)
        passing_g = [line.split("|")[0].replace("PASS:", "").strip() for line in res_g.stdout.split("\n") if line.startswith("PASS:")]

        g_items = []
        for fn in passing_g:
            path = f"{g_dir}/music/{fn}"
            m = compute_10ms_frame_metrics(path)
            if m:
                # Check original source metadata
                meta = get_metadata(path)
                item = {"file": fn, "genre": g_name, "artist": meta["artist"], "title": meta["title"], **m}
                g_items.append(item)
                all_user_items.append(item)

                search_str = f"{fn} {meta['artist']} {meta['title']}".lower()
                if any(kw in search_str for kw in target_keywords):
                    vocal_target_items.append(item)

        results_user_music[g_name] = g_items
        summarize_set(g_items, f"4. USER MUSIC - GENRE: {g_name.upper()}")

    summarize_set(all_user_items, "4. ALL USER MUSIC (95 files)")

    # Print vocal-dominant failures in user music
    print("\n" + "="*80)
    print("VOCAL-DOMINANT / TARGET VOCAL TRACKS IN USER MUSIC")
    print("="*80)
    for x in vocal_target_items:
        print(f"  • {x['genre'].upper():<10} | {x['artist']} - {x['title']:<30} | DEPTH={x['depth_db']:5.2f} dB, P5={x['p5_db']:6.2f} dB, Med={x['median_db']:6.2f} dB | SilFrac={x['silence_fraction']:.4f}, QuietRun={x['longest_quiet_run_ms']:.0f} ms")

    with open("/tmp/depth_metrics_results.json", "w") as f:
        json.dump({
            "set1_pod_correct": results_pod_correct,
            "set1_pod_error": results_pod_error,
            "set2_train_speech": results_train_speech,
            "set3_train_music": results_train_music,
            "set3_classical": results_classical,
            "set4_user_music": results_user_music,
            "vocal_targets": vocal_target_items
        }, f, indent=2)

if __name__ == "__main__":
    main()
