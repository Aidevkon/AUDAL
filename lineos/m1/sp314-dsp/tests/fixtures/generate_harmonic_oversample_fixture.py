import numpy as np
from scipy.signal import firwin
import json, os

OVERSAMPLE = 2
FS         = 48000
FS_UP      = FS * OVERSAMPLE  # 96kHz

# Design FIR: lowpass at 24kHz (Nyquist of 48kHz)
# Normalized cutoff: 24000 / (96000/2) = 0.5
def design_fir(n_taps, cutoff=0.5):
    return firwin(n_taps, cutoff,
                  window='blackmanharris',
                  pass_zero=True)

def measure_attenuation(h, cutoff=0.5):
    from scipy.signal import freqz
    w, H = freqz(h, worN=8192)
    w_norm = w / np.pi
    stopband = np.abs(H[w_norm > 0.6])
    if len(stopband) == 0:
        return 0.0
    return -20.0 * np.log10(np.max(stopband) + 1e-300)

# Find minimum taps for >= 60dB attenuation
best = None
for n_taps in range(7, 128, 2):
    h = design_fir(n_taps)
    att = measure_attenuation(h)
    if att >= 60.0 and best is None:
        best = (n_taps, h, att)
        print(f"Minimum taps: {n_taps}, attenuation: {att:.1f}dB")
        break

n_taps, h, attenuation = best
n_phases     = OVERSAMPLE  # 2
taps_per_phase = (n_taps + n_phases - 1) // n_phases

# Polyphase decomposition
phases_up   = []  # Anti-imaging (scaled x2)
phases_down = []  # Anti-aliasing (no scaling)

for k in range(n_phases):
    phase_up   = []
    phase_down = []
    for i in range(k, n_taps, n_phases):
        coeff = float(h[i])
        if abs(coeff) < 1e-15:
            coeff = 0.0
        phase_up.append(coeff * float(n_phases))
        phase_down.append(coeff)
    while len(phase_up) < taps_per_phase:
        phase_up.append(0.0)
        phase_down.append(0.0)
    phases_up.append(phase_up)
    phases_down.append(phase_down)

# Verify Phase 1 center tap (upsampler ~1.0, decimator ~0.5)
phase1_up   = max(abs(c) for c in phases_up[1])
phase1_down = max(abs(c) for c in phases_down[1])
print(f"Phase 1 max (upsampler): {phase1_up:.6f} (expect ~1.0)")
print(f"Phase 1 max (decimator): {phase1_down:.6f} (expect ~0.5)")

# Verify unity gain of upsampler
print(f"\nUpsampler sum: {sum(sum(p) for p in phases_up):.6f}")
print(f"Decimator sum: {sum(sum(p) for p in phases_down):.6f}")

output = {
    "oversample_factor": OVERSAMPLE,
    "sample_rate":       FS,
    "upsampled_rate":    FS_UP,
    "n_taps":            n_taps,
    "taps_per_phase":    taps_per_phase,
    "stopband_attenuation_db": round(float(attenuation), 2),
    "antiimaging_upsampler": {
        "phases": phases_up,
        "rust_array_type": f"[[f32; {taps_per_phase}]; {n_phases}]"
    },
    "antialiasing_decimator": {
        "phases": phases_down,
        "rust_array_type": f"[[f32; {taps_per_phase}]; {n_phases}]"
    }
}

os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/harmonic_oversample_fir.json", "w") as f:
    json.dump(output, f, indent=2)
print("\nWritten: tests/fixtures/harmonic_oversample_fir.json")
