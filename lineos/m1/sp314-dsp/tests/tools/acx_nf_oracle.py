#!/usr/bin/env python3
# Independent noise-floor oracle for AcxCheckAnalyzer (acx_check.rs).
# Written FROM THE PLUGIN'S DEFINITION (Audacity acx-check.ny), not
# translated from the Rust — that independence is the point: two
# implementations of the same spec agreeing within 0.02 dB is the
# validation. Uses scipy.signal.butter(8, 10, 'highpass') + sosfilt.
# Usage: python3 acx_nf_oracle.py /path/to/file.wav [more.wav ...]
# Requires: numpy, scipy. See acx_check_real_file.rs for the full
# validation record and the sample-file URLs.
"""
Independent verification of the ACX Check noise-floor algorithm.

Implemented FROM the plugin definition (acx-check.ny spec), NOT by
translating the Rust code. Uses scipy.signal.butter for the HP filter.

Algorithm:
  1. Read the WAV verbatim (no resampling). Mono downmix (l+r)/2 if stereo.
  2. 8th-order Butterworth highpass at 10 Hz via scipy.
  3. Square the filtered signal.
  4. Mean over sliding 500 ms windows at 100 ms hop
     (= mean of 5 consecutive 100 ms sub-block mean-squares).
  5. Minimum over all full windows, sqrt, 20*log10.

Also independently computes RMS and peak (DC-removed) for cross-check.
"""

import sys
import wave
import struct
import numpy as np
from scipy.signal import butter, sosfilt

HP_CUTOFF_HZ = 10.0
SUB_BLOCK_MS = 100
WINDOW_SUB_BLOCKS = 5
MIN_SUB_BLOCKS = 10

def read_wav(path):
    """Read WAV, return (samples_float32, sample_rate). Mono downmix if stereo.
    Normalise 16-bit int by dividing by 32768."""
    with wave.open(path, 'rb') as w:
        nch = w.getnchannels()
        sr = w.getframerate()
        sw = w.getsampwidth()
        nframes = w.getnframes()
        raw = w.readframes(nframes)

    if sw == 2:
        samples = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
    elif sw == 3:
        # 24-bit: unpack manually
        samples = np.zeros(nframes * nch, dtype=np.float32)
        for i in range(nframes * nch):
            b = raw[i*3:(i+1)*3]
            val = struct.unpack('<i', b + (b'\xff' if b[2] & 0x80 else b'\x00'))[0]
            samples[i] = val / 8388608.0
    else:
        raise ValueError(f"Unsupported sample width: {sw}")

    if nch == 2:
        samples = (samples[0::2] + samples[1::2]) / 2.0
    elif nch > 2:
        samples = samples.reshape(-1, nch).mean(axis=1)

    return samples, sr

def to_db(linear):
    if linear < 1e-10:
        return -144.0
    return 20.0 * np.log10(linear)

def acx_check(path):
    samples, sr = read_wav(path)
    n = len(samples)

    # --- Peak and RMS (DC-removed) ---
    mean_val = np.mean(samples.astype(np.float64))
    peak = max(float(np.max(samples)) - mean_val, mean_val - float(np.min(samples)))
    mean_sq = np.mean(samples.astype(np.float64)**2) - mean_val**2
    mean_sq = max(mean_sq, 0.0)
    rms = np.sqrt(mean_sq)

    # --- Noise floor ---
    # Step 2: 8th-order Butterworth HP at 10 Hz (scipy)
    sos = butter(8, HP_CUTOFF_HZ, btype='highpass', fs=sr, output='sos')
    filtered = sosfilt(sos, samples).astype(np.float32)

    # Step 3-4: sub-block mean squares, then sliding window
    sub_block_size = int(sr * SUB_BLOCK_MS / 1000)
    n_full_sub_blocks = len(filtered) // sub_block_size

    if n_full_sub_blocks < MIN_SUB_BLOCKS:
        noise_floor_db = None
        top5 = []
    else:
        # Compute mean-square per sub-block
        sub_mean_sqs = np.zeros(n_full_sub_blocks, dtype=np.float64)
        for i in range(n_full_sub_blocks):
            block = filtered[i * sub_block_size : (i + 1) * sub_block_size].astype(np.float64)
            sub_mean_sqs[i] = np.mean(block ** 2)

        # Sliding window: mean of WINDOW_SUB_BLOCKS consecutive sub-block mean-squares
        n_windows = n_full_sub_blocks - WINDOW_SUB_BLOCKS + 1
        window_means = np.zeros(n_windows, dtype=np.float64)
        for i in range(n_windows):
            window_means[i] = np.mean(sub_mean_sqs[i : i + WINDOW_SUB_BLOCKS])

        # Step 5: minimum, sqrt, dB
        min_idx = np.argmin(window_means)
        min_val = window_means[min_idx]
        noise_floor_db = to_db(float(np.sqrt(min_val)))

        # Top 5 smallest windows with time positions
        sorted_indices = np.argsort(window_means)[:5]
        top5 = [(int(idx), float(idx * SUB_BLOCK_MS / 1000.0), float(window_means[idx]),
                 to_db(float(np.sqrt(window_means[idx]))))
                for idx in sorted_indices]

    return {
        'peak_db': to_db(peak),
        'rms_db': to_db(rms),
        'noise_floor_db': noise_floor_db,
        'top5': top5,
        'sr': sr,
        'n_samples': n,
        'n_sub_blocks': n_full_sub_blocks if n_full_sub_blocks >= MIN_SUB_BLOCKS else 0,
    }

# Expected Rust outputs
RUST_EXPECTED = {
    'narration_dream.wav':    {'noise_floor_db': -74.32},
    'narration_crossing.wav': {'noise_floor_db': -94.58},
}

def _rust_nf_for(path):
    """Look up expected Rust noise floor by basename."""
    import os
    base = os.path.basename(path)
    entry = RUST_EXPECTED.get(base)
    return entry['noise_floor_db'] if entry else None

if __name__ == '__main__':
    files = sys.argv[1:] if len(sys.argv) > 1 else [
        '/tmp/narration_dream.wav',
        '/tmp/narration_crossing.wav',
    ]

    for path in files:
        print(f"\n{'='*60}")
        print(f"FILE: {path}")
        print(f"{'='*60}")
        result = acx_check(path)
        print(f"  Sample rate:    {result['sr']} Hz")
        print(f"  Samples:        {result['n_samples']}")
        print(f"  Sub-blocks:     {result['n_sub_blocks']}")
        print(f"  Peak (dBFS):    {result['peak_db']:.2f}")
        print(f"  RMS (dBFS):     {result['rms_db']:.2f}")

        if result['noise_floor_db'] is not None:
            nf = result['noise_floor_db']
            rust_nf = _rust_nf_for(path)
            delta = nf - rust_nf if rust_nf is not None else float('nan')
            print(f"  Noise floor:    {nf:.2f} dBFS")
            if rust_nf is not None:
                print(f"  Rust expected:  {rust_nf:.2f} dBFS")
                print(f"  DELTA:          {delta:+.2f} dB  {'PASS' if abs(delta) <= 0.5 else 'DISAGREE'}")
        else:
            print(f"  Noise floor:    None (input too short)")

        if result['top5']:
            print(f"\n  5 smallest windows (mean-sq, time, dB):")
            print(f"  {'Rank':<6} {'WinIdx':<8} {'Time(s)':<10} {'MeanSq':<14} {'dBFS':<10}")
            print(f"  {'-'*5:<6} {'-'*7:<8} {'-'*9:<10} {'-'*13:<14} {'-'*9:<10}")
            for rank, (idx, t, ms, db) in enumerate(result['top5'], 1):
                print(f"  {rank:<6} {idx:<8} {t:<10.2f} {ms:<14.6e} {db:<10.2f}")
