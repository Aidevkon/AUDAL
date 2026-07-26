#!/usr/bin/env python3
import os
import subprocess
import json
import numpy as np
import re
import shutil
import html
import concurrent.futures

def get_audio_info(fpath):
    cmd = ['ffprobe', '-v', 'quiet', '-print_format', 'json', '-show_format', '-show_streams', fpath]
    try:
        res = subprocess.check_output(cmd, text=True, timeout=10)
        info = json.loads(res)
        fmt = info.get('format', {})
        duration = float(fmt.get('duration', 0.0))
        streams = info.get('streams', [])
        channels = 2
        sample_rate = 48000
        for s in streams:
            if s.get('codec_type') == 'audio':
                channels = int(s.get('channels', 2))
                sample_rate = int(s.get('sample_rate', 48000))
                break
        return {"duration": duration, "channels": channels, "sample_rate": sample_rate}
    except Exception:
        return {"duration": 0.0, "channels": 2, "sample_rate": 48000}

def download_and_extract(item_data):
    idx, feed, dl_dir, speech_dir = item_data
    show_name = feed["show_name"]
    audio_url = html.unescape(feed["audio_url"])
    lang = feed["language"]
    vtype = feed["voice_type"]
    ep_title = feed["ep_title"]

    raw_file = os.path.join(dl_dir, f"ep_{idx}.audio")
    
    # 1. Curl download (first 3MB or full if range fails)
    curl_cmd = [
        "curl", "-sL",
        "-A", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        "-r", "0-3000000",
        "--connect-timeout", "6",
        "--max-time", "30",
        "-o", raw_file,
        audio_url
    ]

    try:
        subprocess.run(curl_cmd, check=True)
        if not os.path.exists(raw_file) or os.path.getsize(raw_file) < 2000:
            curl_cmd_full = [
                "curl", "-sL",
                "-A", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
                "--connect-timeout", "6",
                "--max-time", "35",
                "-o", raw_file,
                audio_url
            ]
            subprocess.run(curl_cmd_full, check=True)

        if not os.path.exists(raw_file) or os.path.getsize(raw_file) < 2000:
            print(f"  FAILED (Curl download empty/small): {show_name}")
            return ("failed", {"show_name": show_name, "reason": "Curl download empty/small"})

        info = get_audio_info(raw_file)
        duration = info["duration"]
        channels = info["channels"]
        sample_rate = info["sample_rate"]

        # With 3MB partial download, use -ss 30 or -ss 60 to safely stay within the downloaded buffer
        if duration > 120.0 or duration == 0.0:
            ss_arg = "60"
        else:
            ss_arg = "10"

        wav_filename = f"speech_{idx}.wav"
        out_wav_path = os.path.join(speech_dir, wav_filename)

        ffmpeg_cmd = [
            "ffmpeg", "-y",
            "-ss", ss_arg,
            "-i", raw_file,
            "-t", "30",
            "-ar", "48000",
            "-ac", "2",
            "-c:a", "pcm_s16le",
            out_wav_path
        ]

        subprocess.run(ffmpeg_cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True, timeout=15)

        if os.path.exists(out_wav_path) and os.path.getsize(out_wav_path) > 1000:
            res_item = {
                "idx": idx,
                "wav_name": wav_filename,
                "wav_path": out_wav_path,
                "show_name": show_name,
                "language": lang,
                "voice_type": vtype,
                "ep_title": ep_title,
                "duration_sec": duration,
                "channels": channels,
                "sample_rate": sample_rate,
                "ss_offset": f"-ss {ss_arg}"
            }
            print(f"  EXTRACTED: {show_name:<35} | Dur: {duration/60.0:5.1f}m | -ss {ss_arg:<3} | Ch: {channels}")
            return ("success", res_item)
        else:
            print(f"  FAILED (FFmpeg empty): {show_name}")
            return ("failed", {"show_name": show_name, "reason": "FFmpeg output empty"})

    except Exception as e:
        print(f"  FAILED: {show_name} ({e})")
        return ("failed", {"show_name": show_name, "reason": str(e)})

def main():
    json_path = "/tmp/podcast_30_feeds.json"
    with open(json_path, "r") as f:
        feeds = json.load(f)

    tmp_dir = "/tmp/podcast_heldout"
    dl_dir = os.path.join(tmp_dir, "raw")
    speech_dir = os.path.join(tmp_dir, "speech")
    if os.path.exists(tmp_dir):
        shutil.rmtree(tmp_dir)
    os.makedirs(dl_dir, exist_ok=True)
    os.makedirs(speech_dir, exist_ok=True)

    print("============================================================")
    print("STEP 1: CURL DOWNLOAD & FFMPEG EXTRACTION OF PODCAST FEEDS")
    print("============================================================")

    tasks = [(idx, feed, dl_dir, speech_dir) for idx, feed in enumerate(feeds)]

    extracted_files = []
    failed_downloads = []

    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as executor:
        results_pool = list(executor.map(download_and_extract, tasks))

    for status, data in results_pool:
        if status == "success":
            extracted_files.append(data)
        else:
            failed_downloads.append(data)

    extracted_files = sorted(extracted_files, key=lambda x: x["idx"])

    print(f"\nTotal Successfully Extracted Clips: {len(extracted_files)} / {len(feeds)}")

    # STEP 2: GATE CORPUS
    print("\n============================================================")
    print("STEP 2: RUNNING GATE (gate_corpus.py --allow-mono)")
    print("============================================================")

    gate_script = "/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py"
    res_gate = subprocess.run([gate_script, tmp_dir, "--allow-mono"], stdout=subprocess.PIPE, text=True)

    passed_files = []
    rejected_files = []

    for line in res_gate.stdout.split("\n"):
        if line.startswith("PASS:"):
            fn = line.split("|")[0].replace("PASS:", "").strip()
            passed_files.append(fn)
            print(f"  {line}")
        elif line.startswith("REJECT:"):
            rejected_files.append(line)
            print(f"  {line}")

    # STEP 3: TRAIN CENTROIDS ON ORIGINAL 40-FILE CORPUS ONLY
    print("\n============================================================")
    print("STEP 3: COMPUTING 40-FILE CORPUS CENTROIDS (ZERO REFITTING)")
    print("============================================================")

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

    print(f"40-FILE CORPUS CENTROIDS:")
    print(f"  Music Centroid:  CV_mean={m_m_cv:.4f}, Flux_mean={m_m_fl:.4f} -> Z: {c_m_z}")
    print(f"  Speech Centroid: CV_mean={m_s_cv:.4f}, Flux_mean={m_s_fl:.4f} -> Z: {c_s_z}")
    print(f"  Pooled Stds:     std_CV={std_cv:.4f}, std_Flux={std_fl:.4f}")

    # STEP 4: MEASURE & CLASSIFY HELD-OUT PODCAST CLIPS
    print("\n============================================================")
    print("STEP 4: MEASURING & CLASSIFYING PODCAST HELD-OUT CORPUS")
    print("============================================================")

    results = []

    for item in extracted_files:
        fn = item["wav_name"]
        if fn not in passed_files:
            continue
        wav_path = item["wav_path"]

        # Measure CV
        out_v = subprocess.check_output([bin_var, wav_path, "0.20"], text=True).strip()
        cvs = [json.loads(l)["cv"] for l in out_v.split("\n") if l.strip()]
        cv_val = float(np.median(cvs)) if cvs else 0.0

        # Measure Flux
        out_m = subprocess.check_output([bin_mfcc, wav_path], text=True).strip()
        flux_val = float(json.loads(out_m)["flux_median"]) if out_m else 0.0

        # Z-score against trained stds
        z_item = np.array([cv_val / std_cv, flux_val / std_fl])
        d_m = float(np.linalg.norm(z_item - c_m_z))
        d_s = float(np.linalg.norm(z_item - c_s_z))

        pred = "speech" if d_s <= d_m else "music"

        res_dict = {**item, "cv": cv_val, "flux": flux_val, "pred": pred, "d_m": d_m, "d_s": d_s}
        results.append(res_dict)

    # OUTPUT REPORT
    n_total = len(results)
    n_speech = sum(1 for x in results if x["pred"] == "speech")
    n_music = sum(1 for x in results if x["pred"] == "music")

    all_cvs = [x["cv"] for x in results]
    all_fls = [x["flux"] for x in results]

    print("\n============================================================")
    print("FINAL PODCAST HELD-OUT TEST SUMMARY")
    print("============================================================")
    print(f"Total Passed Podcast Clips: {n_total}")
    print(f"Classified as SPEECH:       {n_speech} / {n_total} ({n_speech/n_total*100:.2f}%)")
    print(f"Classified as MUSIC:        {n_music} / {n_total} ({n_music/n_total*100:.2f}%)")

    print("\nMETRIC SPREAD ACROSS HELD-OUT PODCAST SET:")
    print(f"  CV (0.20):   Min = {np.min(all_cvs):.4f}, Median = {np.median(all_cvs):.4f}, Max = {np.max(all_cvs):.4f}")
    print(f"  Flux:        Min = {np.min(all_fls):.4f}, Median = {np.median(all_fls):.4f}, Max = {np.max(all_fls):.4f}")

    if n_music > 0:
        print("\nFILES CLASSIFIED AS MUSIC (FALSE NEGATIVES FOR SPEECH ROUTER):")
        for x in results:
            if x["pred"] == "music":
                print(f"  • {x['wav_name']} | {x['show_name']:<35} | CV={x['cv']:.4f}, Flux={x['flux']:.4f} | d_m={x['d_m']:.2f}, d_s={x['d_s']:.2f}")

    with open("/tmp/podcast_results.json", "w") as f:
        json.dump({
            "extracted": extracted_files,
            "failed": failed_downloads,
            "results": results,
            "summary": {
                "n_total": n_total,
                "n_speech": n_speech,
                "n_music": n_music,
                "cv_min": float(np.min(all_cvs)), "cv_med": float(np.median(all_cvs)), "cv_max": float(np.max(all_cvs)),
                "fl_min": float(np.min(all_fls)), "fl_med": float(np.median(all_fls)), "fl_max": float(np.max(all_fls))
            }
        }, f, indent=2)

if __name__ == "__main__":
    main()
