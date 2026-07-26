#!/usr/bin/env python3
import os
import sys
import numpy as np
import wave

def analyze_quiet_frames(filepath):
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
    
    if n_channels > 1:
        data = data.reshape(-1, n_channels).mean(axis=1)

    frame_len = int(framerate * 0.010) # 10ms frame
    num_frames = len(data) // frame_len

    frames = data[:num_frames * frame_len].reshape(num_frames, frame_len)
    rms = np.sqrt(np.mean(frames**2, axis=1))
    eps = 1e-12
    rms_db = 20.0 * np.log10(rms + eps)

    median_db = float(np.median(rms_db))
    p5_db = float(np.percentile(rms_db, 5))
    depth = median_db - p5_db

    thresh_25 = median_db - 25.0
    is_quiet = (rms_db < thresh_25)

    # Find runs of quiet frames
    runs = []
    curr_start = None
    curr_len = 0
    for idx, q in enumerate(is_quiet):
        if q:
            if curr_start is None:
                curr_start = idx
            curr_len += 1
        else:
            if curr_start is not None:
                runs.append((curr_start, curr_len))
                curr_start = None
                curr_len = 0
    if curr_start is not None:
        runs.append((curr_start, curr_len))

    longest_run_frames = max([r[1] for r in runs]) if runs else 0
    longest_run_ms = longest_run_frames * 10.0

    # Edge analysis: quiet frames within first 1s or last 1s
    n_head_quiet = sum(is_quiet[:100])
    n_tail_quiet = sum(is_quiet[-100:])
    n_mid_quiet = sum(is_quiet[100:-100])

    return {
        "filename": os.path.basename(filepath),
        "median_db": median_db,
        "p5_db": p5_db,
        "depth_db": depth,
        "total_frames": num_frames,
        "quiet_frames_count": sum(is_quiet),
        "longest_run_ms": longest_run_ms,
        "runs": runs,
        "head_quiet_frames (first 1s)": n_head_quiet,
        "tail_quiet_frames (last 1s)": n_tail_quiet,
        "mid_quiet_frames (middle)": n_mid_quiet
    }

for fn in ["speech_16.wav", "speech_25.wav"]:
    p = f"/tmp/podcast_heldout/speech/{fn}"
    res = analyze_quiet_frames(p)
    print(f"\n============================================================")
    print(f"FILE: {res['filename']}")
    print(f"============================================================")
    print(f"  Median Level:     {res['median_db']:.2f} dB")
    print(f"  p5 Level:         {res['p5_db']:.2f} dB")
    print(f"  DEPTH:            {res['depth_db']:.2f} dB")
    print(f"  Longest Quiet Run: {res['longest_run_ms']:.0f} ms")
    print(f"  Quiet Frame Dist: Head(1s)={res['head_quiet_frames (first 1s)']} | Tail(1s)={res['tail_quiet_frames (last 1s)']} | Mid={res['mid_quiet_frames (middle)']}")
    print(f"  Total Quiet Runs: {len(res['runs'])}")
    print(f"  Top 5 Longest Runs:")
    top_runs = sorted(res['runs'], key=lambda r: r[1], reverse=True)[:5]
    for start, r_len in top_runs:
        start_ms = start * 10.0
        dur_ms = r_len * 10.0
        is_edge = "HEAD" if start < 100 else ("TAIL" if start + r_len >= res['total_frames'] - 100 else "MID")
        print(f"    - Position: {start_ms/1000.0:5.2f}s to {(start_ms+dur_ms)/1000.0:5.2f}s | Duration: {dur_ms:6.0f} ms | Loc: {is_edge}")
