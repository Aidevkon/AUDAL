#!/usr/bin/env python3
"""
TOOL: gate_corpus.py
PURPOSE: Mechanical corpus admission control (fake-stereo detection, run-length clipping, envelope-correlation duplicate detection).
USAGE: Takes paths to corpus lists or audio files to admit/reject.
REFERENCE: Documented in F-041.
"""
"""
RATIONALE: axis A measures onset timing and works identically on mono. 
Axis D is already excluded from this calibration because the source material is 
lopsided (19/30 speech was mono-duplicated, 1/30 music). So the stereo requirement 
costs us the entire audiobook category — which is the slow-rhythm extreme we most need — 
and buys nothing.
"""
import os
import sys
import json
import subprocess
import numpy as np

def get_ffprobe_info(filepath):
    cmd = [
        "ffprobe", "-v", "error", "-select_streams", "a:0",
        "-show_entries", "stream=sample_rate,channels,duration",
        "-of", "json", filepath
    ]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, text=True)
    info = json.loads(res.stdout).get("streams", [{}])[0]
    return info

def read_audio(filepath):
    cmd = [
        "ffmpeg", "-i", filepath, "-f", "f32le", "-acodec", "pcm_f32le", "-"
    ]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    audio = np.frombuffer(res.stdout, dtype=np.float32)
    if len(audio) % 2 != 0:
        audio = audio[:len(audio) - (len(audio)%2)]
    return audio.reshape(-1, 2)

def compute_envelope(mono, sample_rate=48000, block_ms=100, target_blocks=300):
    block_samples = int(sample_rate * (block_ms / 1000.0))
    num_blocks = min(target_blocks, len(mono) // block_samples)
    env = np.zeros(target_blocks)
    for i in range(num_blocks):
        env[i] = np.sqrt(np.mean(mono[i*block_samples:(i+1)*block_samples]**2))
    env = env - np.mean(env)
    return env

def xcorr_max(x, y):
    nx = np.linalg.norm(x)
    ny = np.linalg.norm(y)
    if nx == 0 or ny == 0: return 0
    corr = np.correlate(x, y, mode='full')
    return np.max(corr) / (nx * ny)

def count_flat_runs(channel_data, thresh=0.999, run_len=8):
    is_clip = np.abs(channel_data) >= thresh
    padded = np.concatenate(([False], is_clip, [False]))
    diff = np.diff(padded.astype(int))
    starts = np.where(diff == 1)[0]
    ends = np.where(diff == -1)[0]
    lengths = ends - starts
    return np.sum(lengths >= run_len), np.sum(is_clip)

def main():
    if len(sys.argv) < 2:
        print("Usage: gate_corpus.py <directory> [--allow-mono]")
        sys.exit(1)
        
    corpus_dir = sys.argv[1]
    allow_mono = "--allow-mono" in sys.argv
    
    if allow_mono:
        print("MONO ALLOWED — this set is valid for axis A only, NOT for axis D or the VAD mid/side sensor.")
        print("-" * 80)
        
    accepted_files = [] 
    classes = ["music", "speech"]
    
    crest_factors = {"music": [], "speech": []}
    
    for cls in classes:
        cls_dir = os.path.join(corpus_dir, cls)
        if not os.path.isdir(cls_dir):
            continue
            
        print(f"\nProcessing class: {cls}")
        files = sorted([f for f in os.listdir(cls_dir) if f.endswith(".wav")])
        accepted_envelopes = []
        
        for f in files:
            filepath = os.path.join(cls_dir, f)
            info = get_ffprobe_info(filepath)
            
            sr = int(info.get("sample_rate", 0))
            ch = int(info.get("channels", 0))
            try: duration = float(info.get("duration", 0))
            except: duration = 0.0
                
            if sr != 48000 or ch != 2:
                print(f"REJECT: {f} | SR/CH | sr={sr}, ch={ch}")
                continue
                
            if abs(duration - 30.0) > 0.1:
                print(f"REJECT: {f} | DURATION | dur={duration:.2f}s")
                continue
                
            audio = read_audio(filepath)
            if len(audio) == 0:
                print(f"REJECT: {f} | EMPTY AUDIO")
                continue
                
            mono = np.mean(audio, axis=1)
            side = (audio[:, 0] - audio[:, 1]) / 2.0
            
            mean_vol_linear = np.sqrt(np.mean(audio**2))
            mean_vol = 20 * np.log10(mean_vol_linear + 1e-10)
            
            side_vol_linear = np.sqrt(np.mean(side**2))
            side_energy = 20 * np.log10(side_vol_linear + 1e-10)
            
            peak_val = np.max(np.abs(audio))
            peak_db = 20 * np.log10(peak_val + 1e-10)
            
            crest_factor = peak_db - mean_vol
            
            runs_ch0, clips_ch0 = count_flat_runs(audio[:, 0])
            runs_ch1, clips_ch1 = count_flat_runs(audio[:, 1])
            total_runs = runs_ch0 + runs_ch1
            total_clips = clips_ch0 + clips_ch1
            
            if side_energy <= -60.0:
                if not allow_mono:
                    print(f"REJECT: {f} | MONO TRAP | side={side_energy:.1f}dB, peak={peak_db:.1f}dB, total_clips={total_clips}, runs={total_runs}, crest={crest_factor:.1f}dB")
                    continue
                else:
                    pass # skip rejection
                
            if mean_vol <= -40.0:
                print(f"REJECT: {f} | QUIET INTRO | mean={mean_vol:.1f}dB, peak={peak_db:.1f}dB, total_clips={total_clips}, runs={total_runs}, crest={crest_factor:.1f}dB")
                continue
                
            if total_runs > 50:
                print(f"REJECT: {f} | RUN-LENGTH CLIPPING | peak={peak_db:.1f}dB, total_clips={total_clips}, runs={total_runs}, crest={crest_factor:.1f}dB")
                continue
                
            env = compute_envelope(mono)
            max_corr = 0.0
            is_dup = False
            for prev_f, prev_env in accepted_envelopes:
                c = xcorr_max(env, prev_env)
                if c > max_corr: max_corr = c
                if c > 0.90:
                    print(f"REJECT: {f} | DUPLICATE | corr={c:.3f} with {prev_f} | mean={mean_vol:.1f}dB, peak={peak_db:.1f}dB, side={side_energy:.1f}dB, total_clips={total_clips}, runs={total_runs}, crest={crest_factor:.1f}dB")
                    is_dup = True
                    break
                    
            if is_dup: continue
                
            accepted_envelopes.append((f, env))
            accepted_files.append((filepath, env))
            crest_factors[cls].append((f, crest_factor))
            print(f"PASS: {f} | dur={duration:.2f}s ch={ch} mean={mean_vol:.1f}dB peak={peak_db:.1f}dB side={side_energy:.1f}dB max_corr={max_corr:.3f} | total_clips={total_clips}, runs={total_runs}, crest={crest_factor:.1f}dB")
            
        n_acc = len(accepted_envelopes)
        high_corr_pairs = 0
        total_pairs = 0
        for i in range(n_acc):
            for j in range(i+1, n_acc):
                total_pairs += 1
                c = xcorr_max(accepted_envelopes[i][1], accepted_envelopes[j][1])
                if c > 0.70: high_corr_pairs += 1
                    
        print(f"\n--- CLASS REPORT: {cls} ---")
        print(f"Accepted files: {n_acc}")
        print(f"Pairs exceeding 0.7 correlation: {high_corr_pairs} out of {total_pairs} pairs.")

    print("\n" + "="*40)
    for cls in classes:
        crests = sorted([c[1] for c in crest_factors[cls]])
        if not crests: continue
        min_c, median_c, max_c = np.min(crests), np.median(crests), np.max(crests)
        print(f"\n--- CREST FACTOR DISTRIBUTION: {cls} ---")
        print(f"Min: {min_c:.1f} dB | Median: {median_c:.1f} dB | Max: {max_c:.1f} dB")
        
        cf_sorted = sorted(crest_factors[cls], key=lambda x: x[1])
        visited = set()
        
        for i in range(len(cf_sorted)):
            if i in visited: continue
            group = [cf_sorted[i]]
            for j in range(i+1, len(cf_sorted)):
                if abs(cf_sorted[i][1] - cf_sorted[j][1]) <= 2.0:
                    group.append(cf_sorted[j])
                else:
                    break
            
            if len(group) >= 3:
                for idx in range(i, i+len(group)): visited.add(idx)
                files_str = ", ".join([f[0] for f in group])
                vals_str = ", ".join([f"{f[1]:.1f}dB" for f in group])
                print(f"  [FLAG] Monoculture Signature (within 2dB): {vals_str}")
                print(f"         Files: {files_str}")

if __name__ == "__main__":
    main()
