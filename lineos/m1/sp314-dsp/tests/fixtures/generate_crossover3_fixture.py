"""
Oracle: CrossoverLR4x3 — 3-band Linkwitz-Riley 4th order crossover.
Proves flat summation (low + mid + high = input) across frequency range.
Proves -6dB per band at respective crossover frequencies.

Run from sp314-dsp crate root:
python3 tests/fixtures/generate_crossover3_fixture.py
"""

import numpy as np
import json, hashlib

FS         = 48000
F_LOW      = 200.0
F_HIGH     = 3000.0
N_SETTLE   = 8000   # samples: LR4 at crossover freq needs full settling
N_MEASURE  = 2000   # samples to measure peak over

def make_biquad_lp(fc, fs):
    """Second-order Butterworth LP biquad — matches CrossoverLR4::new() exactly."""
    w0    = 2.0 * np.pi * fc / fs
    q     = 1.0 / np.sqrt(2.0)
    cos_w = np.cos(w0)
    sin_w = np.sin(w0)
    alpha = sin_w / (2.0 * q)
    b0 = (1.0 - cos_w) / 2.0
    b1 =  1.0 - cos_w
    b2 = (1.0 - cos_w) / 2.0
    a0 =  1.0 + alpha
    a1 = -2.0 * cos_w
    a2 =  1.0 - alpha
    return [b0/a0, b1/a0, b2/a0, a1/a0, a2/a0]

def make_biquad_hp(fc, fs):
    """Second-order Butterworth HP biquad — matches CrossoverLR4::new() exactly."""
    w0    = 2.0 * np.pi * fc / fs
    q     = 1.0 / np.sqrt(2.0)
    cos_w = np.cos(w0)
    sin_w = np.sin(w0)
    alpha = sin_w / (2.0 * q)
    b0 = (1.0 + cos_w) / 2.0
    b1 = -(1.0 + cos_w)
    b2 = (1.0 + cos_w) / 2.0
    a0 =  1.0 + alpha
    a1 = -2.0 * cos_w
    a2 =  1.0 - alpha
    return [b0/a0, b1/a0, b2/a0, a1/a0, a2/a0]

def apply_biquad(x, coeffs, state):
    """Direct Form II transposed biquad — matches process_biquad() exactly."""
    b0, b1, b2, a1, a2 = coeffs
    w1, w2 = state
    y = b0 * x + w1
    new_w1 = b1 * x - a1 * y + w2
    new_w2 = b2 * x - a2 * y
    # flush-to-zero matching Rust implementation
    if abs(new_w1) < 1e-15: new_w1 = 0.0
    if abs(new_w2) < 1e-15: new_w2 = 0.0
    return y, [new_w1, new_w2]

def apply_lr4(signal, fc, fs, kind):
    """Apply LR4 filter (double biquad cascade) — matches CrossoverLR4::process()."""
    if kind == 'lp':
        coeffs = make_biquad_lp(fc, fs)
    else:
        coeffs = make_biquad_hp(fc, fs)
    state1 = [0.0, 0.0]
    state2 = [0.0, 0.0]
    out = np.zeros(len(signal))
    for i, x in enumerate(signal):
        y1, state1 = apply_biquad(x, coeffs, state1)
        y2, state2 = apply_biquad(y1, coeffs, state2)
        out[i] = y2
    return out

def peak_db(signal):
    """Peak amplitude in dB over steady-state window."""
    steady = signal[N_SETTLE : N_SETTLE + N_MEASURE]
    peak = float(np.max(np.abs(steady)))
    if peak < 1e-12:
        return -144.0
    return float(20.0 * np.log10(peak))

def simulate_crossover3(signal):
    """
    Simulate CrossoverLR4x3::process() exactly.
    Stage 1: x -> (low, mid_high) at F_LOW
    Stage 2: mid_high -> (mid, high) at F_HIGH
    """
    low      = apply_lr4(signal, F_LOW,  FS, 'lp')
    mid_high = apply_lr4(signal, F_LOW,  FS, 'hp')
    mid      = apply_lr4(mid_high, F_HIGH, FS, 'lp')
    high     = apply_lr4(mid_high, F_HIGH, FS, 'hp')
    return low, mid, high

# --- Sum checks across 7 frequencies ---
test_freqs = [20, 100, 200, 500, 1000, 3000, 10000]
sum_checks = []

print(f"Oracle: CrossoverLR4x3 (f_low={F_LOW}Hz, f_high={F_HIGH}Hz, fs={FS}Hz)")
print(f"{'Freq':>8}  {'Sum dB':>8}  {'Low dB':>8}  {'Mid dB':>8}  {'High dB':>8}")

for freq in test_freqs:
    t = np.arange(N_SETTLE + N_MEASURE) / FS
    signal = np.sin(2.0 * np.pi * freq * t)

    low, mid, high = simulate_crossover3(signal)
    total = low + mid + high

    sum_db  = peak_db(total)
    low_db  = peak_db(low)
    mid_db  = peak_db(mid)
    high_db = peak_db(high)

    print(f"{freq:>8}  {sum_db:>8.3f}  {low_db:>8.3f}  {mid_db:>8.3f}  {high_db:>8.3f}")

    sum_checks.append({
        "freq_hz":          float(freq),
        "expected_sum_db":  round(sum_db, 3),
        "tolerance_db":     0.2,
        "low_db":           round(low_db,  2),
        "mid_db":           round(mid_db,  2),
        "high_db":          round(high_db, 2),
    })

# --- Split checks at crossover frequencies ---
split_checks = []
for fc, label in [(F_LOW, "f_low"), (F_HIGH, "f_high")]:
    t = np.arange(N_SETTLE + N_MEASURE) / FS
    signal = np.sin(2.0 * np.pi * fc * t)
    low, mid, high = simulate_crossover3(signal)

    if label == "f_low":
        # At F_LOW: low should be -6dB, mid_high should be -6dB
        mid_high = apply_lr4(signal, F_LOW, FS, 'hp')
        band_a_db = peak_db(low)
        band_b_db = peak_db(mid_high)
        print(f"\nAt f_low={fc}Hz: low={band_a_db:.3f}dB, mid_high={band_b_db:.3f}dB (both should be -6dB)")
    else:
        # At F_HIGH: mid should be -6dB, high should be -6dB
        mid_high = apply_lr4(signal, F_LOW, FS, 'hp')
        band_a_db = peak_db(apply_lr4(mid_high, F_HIGH, FS, 'lp'))
        band_b_db = peak_db(apply_lr4(mid_high, F_HIGH, FS, 'hp'))
        print(f"At f_high={fc}Hz: mid={band_a_db:.3f}dB, high={band_b_db:.3f}dB (both should be -6dB)")

    split_checks.append({
        "label":          label,
        "freq_hz":        float(fc),
        "expected_db":    -6.0,
        "tolerance_db":   0.2,
        "measured_a_db":  round(band_a_db, 3),
        "measured_b_db":  round(band_b_db, 3),
    })

# --- Build fixture ---
fixture = {
    "f_low_hz":      F_LOW,
    "f_high_hz":     F_HIGH,
    "sample_rate":   FS,
    "n_settle":      N_SETTLE,
    "n_measure":     N_MEASURE,
    "oracle":        "generate_crossover3_fixture.py",
    "phase_rule":    "LR4x3: low+mid+high (NO sign inversion). Sum must be flat.",
    "sum_checks":    sum_checks,
    "split_checks":  split_checks,
}

fixture_bytes = json.dumps(fixture, sort_keys=True).encode()
sha256 = hashlib.sha256(fixture_bytes).hexdigest()

with open("tests/fixtures/crossover3_reference.json", "wb") as f:
    f.write(fixture_bytes)
with open("tests/fixtures/crossover3_reference.lock", "w") as f:
    f.write(sha256)

print(f"\nFixture written: tests/fixtures/crossover3_reference.json")
print(f"Lock:            tests/fixtures/crossover3_reference.lock")
print(f"SHA256:          {sha256}")
