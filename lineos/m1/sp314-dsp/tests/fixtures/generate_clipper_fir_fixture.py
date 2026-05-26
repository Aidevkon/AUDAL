import numpy as np
from scipy.signal import firwin
from scipy.signal.windows import blackmanharris
import json, os

OVERSAMPLE = 4
FS = 48000
FS_UP = FS * OVERSAMPLE  # 192000 Hz

# Both filters have identical spec:
# Lowpass cutoff at Nyquist of original rate = 24000 Hz
# Normalized: 24000 / (192000/2) = 0.25
# We reuse the same FIR design as True Peak detector
# (same cutoff, same window — proven to work)

def design_fir(n_taps, cutoff=0.25):
    return firwin(n_taps, cutoff,
                  window='blackmanharris',
                  pass_zero=True)

def measure_stopband_attenuation(h, cutoff=0.25):
    from scipy.signal import freqz
    w, H = freqz(h, worN=8192)
    w_norm = w / np.pi
    stopband = np.abs(H[w_norm > 0.35])
    if len(stopband) == 0:
        return 0.0
    return -20.0 * np.log10(np.max(stopband) + 1e-300)

# Find minimum taps: >= 60dB stopband attenuation
best = None
for n_taps in range(7, 128, 2):
    h = design_fir(n_taps)
    att = measure_stopband_attenuation(h)
    if att >= 60.0 and best is None:
        best = (n_taps, h, att)
        print(f"Minimum taps: {n_taps}, attenuation: {att:.1f}dB")
        break

n_taps, h, attenuation = best

# Polyphase decomposition with zero-padding
# (same pattern as True Peak detector)
n_phases = OVERSAMPLE  # 4
taps_per_phase = (n_taps + n_phases - 1) // n_phases

flush_count = 0
pad_count = 0
phases_up = []   # Anti-Imaging (upsampler): scaled x4
phases_down = [] # Anti-Aliasing (decimator): no scaling

for k in range(n_phases):
    phase_up = []
    phase_down = []
    for i in range(k, n_taps, n_phases):
        coeff = float(h[i])
        if abs(coeff) < 1e-15:
            coeff = 0.0
            flush_count += 1
        phase_up.append(coeff * float(n_phases))  # x4 for upsampler
        phase_down.append(coeff)                   # no scaling for decimator
    # Zero-pad to uniform length
    while len(phase_up) < taps_per_phase:
        phase_up.append(0.0)
        phase_down.append(0.0)
        pad_count += 1
    phases_up.append(phase_up)
    phases_down.append(phase_down)

assert all(len(p) == taps_per_phase for p in phases_up)
assert all(len(p) == taps_per_phase for p in phases_down)

# Verify upsampler unity gain (Phase 3 center tap should be ~1.0)
phase3_max = max(abs(c) for c in phases_up[3])
print(f"Phase 3 max (upsampler): {phase3_max:.6f} (expect ~1.0)")

# Verify decimator (Phase 3 center tap should be ~0.25)
phase3_max_down = max(abs(c) for c in phases_down[3])
print(f"Phase 3 max (decimator): {phase3_max_down:.6f} (expect ~0.25)")

# Soft clipper polynomial verification:
# f(x) = 1.5 * (x - x^3/3) for |x| <= 1.0
# At x=1.0: f(1.0) = 1.5 * (1.0 - 1/3) = 1.5 * 0.667 = 1.0 (unity ceiling)
# At x=0.5: f(0.5) = 1.5 * (0.5 - 0.125/3) = 1.5 * 0.458 = 0.688
test_points = [0.0, 0.5, 0.8, 1.0]
print("\nSoft clipper f(x) = 1.5*(x - x^3/3):")
for x in test_points:
    x_c = max(-1.0, min(1.0, x))
    y = 1.5 * (x_c - (x_c**3) / 3.0)
    print(f"  f({x:.1f}) = {y:.6f}")

output = {
    "oversample_factor": OVERSAMPLE,
    "sample_rate": FS,
    "upsampled_rate": FS_UP,
    "n_taps": n_taps,
    "taps_per_phase": taps_per_phase,
    "stopband_attenuation_db": round(float(attenuation), 2),
    "antiimaging_upsampler": {
        "description": "Anti-imaging FIR for 4x upsampling (scaled x4)",
        "phases": phases_up,
        "rust_array_type": f"[[f64; {taps_per_phase}]; {n_phases}]"
    },
    "antialiasing_decimator": {
        "description": "Anti-aliasing FIR for 4x decimation (no scaling)",
        "phases": phases_down,
        "rust_array_type": f"[[f64; {taps_per_phase}]; {n_phases}]"
    },
    "soft_clipper": {
        "formula": "1.5 * (x - x^3/3) for |x| <= 1.0",
        "ceiling": 1.0,
        "unity_gain_at_x1": True,
        "rescale_factor": 1.5
    },
    "flush_to_zero_count": flush_count,
    "zero_pad_count": pad_count
}

os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/clipper_fir.json", "w") as f:
    json.dump(output, f, indent=2)
print("\nWritten: tests/fixtures/clipper_fir.json")
