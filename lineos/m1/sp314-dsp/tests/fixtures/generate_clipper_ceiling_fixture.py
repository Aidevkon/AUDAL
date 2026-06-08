"""
Oracle: OversampledSoftClipper ceiling compliance.
Proves that for ceiling = 0.8913 linear (-1.0 dBTP),
no output sample exceeds ceiling after ISP sequence.

Run: python3 tests/fixtures/generate_clipper_ceiling_fixture.py
from the sp314-dsp crate root.
"""

import numpy as np
from scipy.signal import lfilter
import json, hashlib

TAPS_UP = 18
TAPS_DOWN = 16
CEILING_DB = -1.0
CEILING_LINEAR = float(10.0 ** (CEILING_DB / 20.0))

# POLYPHASE_UP — must match true_peak.rs and updated clipper.rs exactly
POLYPHASE_UP = [
    [1.5434028189789954e-06, -7.13001497110598e-05, 0.000547916598168763,
     -0.002394334896663008, 0.00765778050360655, -0.01997707323052987,
     0.045894021621008786, -0.10210040651198932, 0.28774073663114313,
     0.8961254805339471, -0.16014317824914467, 0.06812954776079619,
     -0.030617169014956896, 0.012624472825444186, -0.00441633952898109,
     0.0012025272034240427, -0.00021887745333895077, 1.5049006032051983e-05],
    [6.598318271635479e-06, -0.00018264688010727247, 0.001163958534797638,
     -0.00463817305124454, 0.013984413366646358, -0.03509534960972341,
     0.07909798709722038, -0.17917620537135998, 0.6248379126482765,
     0.6248379126482765, -0.17917620537135998, 0.07909798709722038,
     -0.03509534960972341, 0.013984413366646358, -0.00463817305124454,
     0.001163958534797638, -0.00018264688010727247, 6.598318271635479e-06],
    [1.5049006032051983e-05, -0.00021887745333895077, 0.0012025272034240427,
     -0.00441633952898109, 0.012624472825444186, -0.030617169014956896,
     0.06812954776079619, -0.16014317824914467, 0.8961254805339471,
     0.28774073663114313, -0.10210040651198932, 0.045894021621008786,
     -0.01997707323052987, 0.00765778050360655, -0.002394334896663008,
     0.000547916598168763, -7.13001497110598e-05, 1.5434028189789954e-06],
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
     1.000002215792296,
     0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
]

# POLYPHASE_DOWN — unchanged from clipper.rs
POLYPHASE_DOWN = [
    [-4.356356793601984e-07, 2.7616383116088917e-05, -0.00023526493022940875,
     0.0010919779630639951, -0.0036186260258062883, 0.009737114410200651,
     -0.023937394398821293, 0.07110629238270702, 0.22374306166147695,
     -0.03875888123237064, 0.015292569417395596, -0.006052295063321366,
     0.002054202693303022, -0.0005344364312116799, 8.980023611299895e-05,
     -5.426467339603522e-06],
    [-2.2165091726867164e-06, 7.291410498226346e-05, -0.000509091903940878,
     0.002137591869993243, -0.006655978702492663, 0.017290507750770918,
     -0.04274148106832401, 0.155408237538181, 0.155408237538181,
     -0.04274148106832401, 0.017290507750770918, -0.006655978702492665,
     0.002137591869993244, -0.0005090919039408789, 7.291410498226358e-05,
     -2.2165091726870662e-06],
    [-5.426467339603522e-06, 8.980023611299895e-05, -0.0005344364312116799,
     0.002054202693303023, -0.006052295063321367, 0.015292569417395601,
     -0.03875888123237064, 0.22374306166147695, 0.07110629238270702,
     -0.023937394398821293, 0.009737114410200646, -0.0036186260258062866,
     0.0010919779630639943, -0.00023526493022940875, 2.7616383116088917e-05,
     -4.356356793601984e-07],
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
     0.24999928391481227,
     0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
]

def soft_clip(x, ceiling):
    scale = 1.5 / ceiling
    xc = float(np.clip(x * scale, -1.5, 1.5))
    clipped = xc - (4.0 / 27.0) * (xc ** 3)
    return clipped * ceiling

def simulate_clipper(signal, ceiling):
    """
    Simulate OversampledSoftClipper.process() sample by sample.
    Matches the Rust implementation exactly.
    """
    delay_up = np.zeros(TAPS_UP)
    delay_down = [np.zeros(TAPS_DOWN) for _ in range(4)]
    write_up = 0
    write_down = 0
    outputs = []

    for sample in signal:
        # Write to upsampler delay (backwards circular)
        write_up = (write_up + TAPS_UP - 1) % TAPS_UP
        delay_up[write_up] = sample

        # Upsample: 4 phases + clip
        clipped = [0.0] * 4
        for p in range(4):
            acc = 0.0
            for tap in range(TAPS_UP):
                idx = (write_up + tap) % TAPS_UP
                acc += POLYPHASE_UP[p][tap] * delay_up[idx]
            clipped[p] = soft_clip(acc, ceiling)

        # Write clipped samples to downsampler (reversed order)
        write_down = (write_down + TAPS_DOWN - 1) % TAPS_DOWN
        for p in range(4):
            delay_down[p][write_down] = clipped[3 - p]

        # Downsample: sum all 4 phases
        out = 0.0
        for p in range(4):
            for tap in range(TAPS_DOWN):
                idx = (write_down + tap) % TAPS_DOWN
                out += POLYPHASE_DOWN[p][tap] * delay_down[p][idx]
        outputs.append(out)

    return outputs

# ISP test sequence — 200 repetitions to ensure full settling
pattern = [0.9, 0.9, -0.9, -0.9]
signal = pattern * 200  # 800 samples

outputs = simulate_clipper(signal, CEILING_LINEAR)

# Skip settling period (TAPS_UP + TAPS_DOWN = 34 samples)
settling = TAPS_UP + TAPS_DOWN
steady_outputs = outputs[settling:]

max_output = float(np.max(np.abs(steady_outputs)))
min_output = float(np.min(steady_outputs))

print(f"Ceiling linear:  {CEILING_LINEAR:.6f} ({CEILING_DB} dBTP)")
print(f"Max output:      {max_output:.6f}")
print(f"Ceiling margin:  {(CEILING_LINEAR - max_output):.6f}")
print(f"Ceiling holds:   {max_output <= CEILING_LINEAR + 1e-4}")
print(f"Signal alive:    {max_output > 0.8}")

assert max_output <= CEILING_LINEAR + 1e-4, \
    f"CEILING VIOLATED: {max_output:.6f} > {CEILING_LINEAR:.6f}"
assert max_output > 0.8, \
    f"SIGNAL KILLED: {max_output:.6f}"

fixture = {
    "ceiling_db": CEILING_DB,
    "ceiling_linear": CEILING_LINEAR,
    "isp_sequence": pattern,
    "repetitions": 200,
    "settling_samples": settling,
    "max_output_after_settling": max_output,
    "tolerance": 1e-4,
    "signal_floor": 0.8,
    "oracle": "generate_clipper_ceiling_fixture.py",
    "fir_taps_up": TAPS_UP,
    "fir_taps_down": TAPS_DOWN,
}

fixture_bytes = json.dumps(fixture, sort_keys=True).encode()
sha256 = hashlib.sha256(fixture_bytes).hexdigest()

with open("tests/fixtures/clipper_ceiling_reference.json", "wb") as f:
    f.write(fixture_bytes)
with open("tests/fixtures/clipper_ceiling_reference.lock", "w") as f:
    f.write(sha256)

print(f"\nFixture written.")
print(f"Lock: {sha256}")
