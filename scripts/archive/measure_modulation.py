#!/usr/bin/env python3
import os
import sys
import subprocess
import numpy as np
import json
import re

def read_audio(filepath):
    cmd = [
        "ffmpeg", "-i", filepath,
        "-f", "f32le", "-ac", "2", "-ar", "48000",
        "-loglevel", "error", "-"
    ]
    raw = subprocess.check_output(cmd)
    samples = np.frombuffer(raw, dtype=np.float32)
    stereo = samples.reshape(-1, 2)
    mono = np.mean(stereo, axis=1)
    return mono, 48000

def process_file(filepath):
    mono, sr = read_audio(filepath)
    
    # 2. Envelope: RMS over consecutive 10 ms blocks (480 samples at 48k)
    block_size = int(sr * 0.010)
    n_blocks = len(mono) // block_size
    trimmed = mono[:n_blocks * block_size].reshape(n_blocks, block_size)
    envelope = np.sqrt(np.mean(trimmed**2, axis=1))
    
    # CHANGE 1: Log envelope in dB
    env_db = 20.0 * np.log10(envelope + 1e-10)
    
    fs_env = 100.0
    win_len = int(5.0 * fs_env) # 500
    hop_len = int(1.0 * fs_env) # 100
    
    n_fft = 512
    freqs = np.fft.rfftfreq(n_fft, d=1.0/fs_env)
    
    # Bins
    idx_3_6 = (freqs >= 3.0) & (freqs <= 6.0)
    idx_1_3 = (freqs >= 1.0) & (freqs <= 3.0)
    idx_05_20 = (freqs >= 0.5) & (freqs <= 20.0)
    
    mod_contrasts = []
    peak_mod_hz_list = []
    window_powers = []
    
    start = 0
    hann = np.hanning(win_len)
    
    while start + win_len <= len(env_db):
        win = env_db[start:start+win_len]
        win_dc_removed = win - np.mean(win)
        win_windowed = win_dc_removed * hann
        
        fft_res = np.fft.rfft(win_windowed, n=n_fft)
        power = np.abs(fft_res)**2
        window_powers.append(power)
        
        energy_3_6 = np.sum(power[idx_3_6])
        energy_1_3 = np.sum(power[idx_1_3])
        
        if energy_1_3 > 1e-12:
            mod_contrast = energy_3_6 / energy_1_3
        else:
            mod_contrast = 0.0
            
        power_05_20 = power[idx_05_20]
        freqs_05_20 = freqs[idx_05_20]
        if len(power_05_20) > 0:
            max_idx = np.argmax(power_05_20)
            peak_hz = freqs_05_20[max_idx]
        else:
            peak_hz = 0.0
            
        mod_contrasts.append(mod_contrast)
        peak_mod_hz_list.append(peak_hz)
        
        start += hop_len
        
    if not mod_contrasts:
        return 0.0, 0.0, 0.0, 0.0, None, None
        
    mean_power = np.mean(window_powers, axis=0) if window_powers else None
    return np.min(mod_contrasts), np.median(mod_contrasts), np.max(mod_contrasts), np.median(peak_mod_hz_list), freqs, mean_power

def compute_20_bins(freqs, mean_power):
    # Power in each 0.5 Hz bin from 0.5 to 10.0 Hz (20 bins)
    bin_powers = []
    for i in range(20):
        low = 0.5 + i * 0.5
        high = 0.5 + (i + 1) * 0.5
        mask = (freqs >= low) & (freqs < high)
        p = np.sum(mean_power[mask]) if np.any(mask) else 0.0
        bin_powers.append(p)
    return bin_powers

def find_best_threshold(data):
    sorted_data = sorted(data, key=lambda x: x[3])
    vals = [x[3] for x in sorted_data]
    
    thresholds = []
    thresholds.append(vals[0] - 0.001)
    for i in range(len(vals) - 1):
        thresholds.append((vals[i] + vals[i+1]) / 2.0)
    thresholds.append(vals[-1] + 0.001)
    
    best_t = 0.0
    best_acc = -1.0
    
    for t in thresholds:
        correct = 0
        for f, cat, src, val, peak_hz, freqs, mean_p in data:
            pred = "speech" if val >= t else "music"
            if pred == cat:
                correct += 1
        acc = correct / len(data)
        if acc > best_acc:
            best_acc = acc
            best_t = t
            
    return best_t, best_acc

def main():
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

    # Get passing Set A files
    cmd = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/diverse_corpus", "--allow-mono"]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, text=True)
    passing_A = []
    for line in res.stdout.split("\n"):
        if line.startswith("PASS:"):
            fn = line.split("|")[0].replace("PASS:", "").strip()
            passing_A.append(fn)

    dataset_A = []
    for fn in sorted(passing_A):
        cat = "music" if fn.startswith("music") else "speech"
        path = f"/tmp/diverse_corpus/{cat}/{fn}"
        mi, med, ma, peak_hz, freqs, mean_p = process_file(path)
        src = file_to_source.get(fn, "unknown")
        dataset_A.append((fn, cat, src, med, peak_hz, freqs, mean_p))
        
    # Get passing Set B files
    cmd_B = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/loose_music", "--allow-mono"]
    res_B = subprocess.run(cmd_B, stdout=subprocess.PIPE, text=True)
    passing_B = []
    for line in res_B.stdout.split("\n"):
        if line.startswith("PASS:"):
            fn = line.split("|")[0].replace("PASS:", "").strip()
            passing_B.append(fn)

    dataset_B = []
    for fn in sorted(passing_B):
        path = f"/tmp/loose_music/music/{fn}"
        mi, med, ma, peak_hz, freqs, mean_p = process_file(path)
        dataset_B.append((fn, "music", "loose_classical", med, peak_hz, freqs, mean_p))

    # --- REPORT 1: PER CLASS (SET A) MOD_CONTRAST ---
    music_contrasts_A = [x[3] for x in dataset_A if x[1] == "music"]
    speech_contrasts_A = [x[3] for x in dataset_A if x[1] == "speech"]
    
    m_min, m_p25, m_med, m_p75, m_max = np.min(music_contrasts_A), np.percentile(music_contrasts_A, 25), np.median(music_contrasts_A), np.percentile(music_contrasts_A, 75), np.max(music_contrasts_A)
    s_min, s_p25, s_med, s_p75, s_max = np.min(speech_contrasts_A), np.percentile(speech_contrasts_A, 25), np.median(speech_contrasts_A), np.percentile(speech_contrasts_A, 75), np.max(speech_contrasts_A)
    
    speech_below_music_p75 = np.mean(np.array(speech_contrasts_A) < m_p75)
    music_above_speech_p25 = np.mean(np.array(music_contrasts_A) > s_p25)
    
    print("=== REPORT 1: SET A MOD_CONTRAST SPREAD ===")
    print(f"MUSIC:  Min={m_min:.4f}, p25={m_p25:.4f}, Med={m_med:.4f}, p75={m_p75:.4f}, Max={m_max:.4f}")
    print(f"SPEECH: Min={s_min:.4f}, p25={s_p25:.4f}, Med={s_med:.4f}, p75={s_p75:.4f}, Max={s_max:.4f}")
    print(f"Overlap: Speech below Music p75 ({m_p75:.4f}): {speech_below_music_p75*100:.1f}%")
    print(f"Overlap: Music above Speech p25 ({s_p25:.4f}): {music_above_speech_p25*100:.1f}%")

    # --- REPORT 2: PEAK MODULATION FREQUENCY ON LOG ENVELOPE ---
    music_peaks_A = [x[4] for x in dataset_A if x[1] == "music"]
    speech_peaks_A = [x[4] for x in dataset_A if x[1] == "speech"]
    print("\n=== REPORT 2: PEAK MODULATION FREQUENCY ON LOG ENVELOPE ===")
    print(f"MUSIC (Set A) Median Peak Mod Hz:  {np.median(music_peaks_A):.2f} Hz (p25={np.percentile(music_peaks_A,25):.2f}, p75={np.percentile(music_peaks_A,75):.2f})")
    print(f"SPEECH (Set A) Median Peak Mod Hz: {np.median(speech_peaks_A):.2f} Hz (p25={np.percentile(speech_peaks_A,25):.2f}, p75={np.percentile(speech_peaks_A,75):.2f})")
    print(f"LOOSE CLASSICAL (Set B) Median Peak Mod Hz: {np.median([x[4] for x in dataset_B]):.2f} Hz")

    # --- REPORT 4: LOO ANALYSIS ON SET A ---
    full_t, full_acc = find_best_threshold(dataset_A)
    sources_A = sorted(list(set(x[2] for x in dataset_A)))
    
    loo_results = []
    for src in sources_A:
        subset = [x for x in dataset_A if x[2] != src]
        t, acc = find_best_threshold(subset)
        loo_results.append((src, t, acc))
        
    t_vals = [r[1] for r in loo_results]
    acc_vals = [r[2] for r in loo_results]
    min_t, max_t, spread = np.min(t_vals), np.max(t_vals), np.max(t_vals) - np.min(t_vals)
    
    print("\n=== REPORT 4: LOO ANALYSIS ON MOD_CONTRAST (SET A) ===")
    print(f"Full Set Threshold: {full_t:.4f} (Accuracy: {full_acc*100:.2f}%)")
    for src, t, acc in loo_results:
        print(f"  Removed '{src}': Threshold = {t:.4f} (Acc on rest: {acc*100:.2f}%)")
    print(f"LOO Summary: Min={min_t:.4f}, Max={max_t:.4f}, Spread={spread:.4f}, Acc Range={np.min(acc_vals)*100:.1f}% - {np.max(acc_vals)*100:.1f}%")

    # --- REPORT 3: SET B (LOOSE CLASSICAL) VS BOUNDARY ---
    print("\n=== REPORT 3: SET B (LOOSE CLASSICAL) VS SET A BOUNDARY ===")
    print(f"Set A Boundary Threshold: {full_t:.4f}")
    print(f"{'FILE':<40} | {'MOD_CONTRAST MEDIAN':<20} | {'PEAK_MOD_HZ':<12} | {'SIDE'}")
    print("-" * 90)
    
    b_music_side = 0
    b_speech_side = 0
    for fn, cat, src, med_val, peak_hz, freqs, mean_p in dataset_B:
        side = "MUSIC SIDE (<= thresh)" if med_val <= full_t else "SPEECH SIDE (> thresh)"
        if med_val <= full_t:
            b_music_side += 1
        else:
            b_speech_side += 1
        print(f"{fn:<40} | {med_val:<20.4f} | {peak_hz:<12.2f} Hz | {side}")
        
    print("-" * 90)
    print(f"Set B Result: {b_music_side} / {len(dataset_B)} fell on the MUSIC SIDE.")
    print(f"Set B Result: {b_speech_side} / {len(dataset_B)} fell on the SPEECH SIDE.")

    # --- REPORT 5: 20-BIN LOG-ENVELOPE SPECTRUM FOR ONE SPEECH & ONE ELECTRONIC MUSIC FILE ---
    print("\n=== REPORT 5: 20-BIN LOG-ENVELOPE SPECTRUM (0.5 to 10 Hz) ===")
    speech_sample = [x for x in dataset_A if x[0] == "speech_14.wav"][0]
    music_sample = [x for x in dataset_A if x[0] == "music_11.wav"][0]
    
    sp_bins = compute_20_bins(speech_sample[5], speech_sample[6])
    mu_bins = compute_20_bins(music_sample[5], music_sample[6])
    
    print("Bin Range (Hz) | Speech (speech_14.wav) | Music (music_11.wav)")
    print("-" * 60)
    for i in range(20):
        low = 0.5 + i * 0.5
        high = 0.5 + (i + 1) * 0.5
        print(f"{low:4.1f} - {high:4.1f} Hz | {sp_bins[i]:20.2f} | {mu_bins[i]:20.2f}")

if __name__ == "__main__":
    main()
