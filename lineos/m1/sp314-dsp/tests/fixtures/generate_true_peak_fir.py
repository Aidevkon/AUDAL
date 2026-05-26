import numpy as np
from scipy.signal import firwin, freqz, lfilter
import json, os

OVERSAMPLE = 4
FS = 48000
FS_UP = FS * OVERSAMPLE  # 192000 Hz

def design_fir(n_taps, cutoff=0.25):
    """
    Design FIR with Blackman-Harris window.
    blackmanharris needs no parameters and gives
    >60dB stopband attenuation — no Kaiser beta needed.
    """
    return firwin(n_taps, cutoff, window='blackmanharris',
                  pass_zero=True)

def measure_stopband_attenuation(h, cutoff=0.25):
    """Measure attenuation at 0.35 (stopband)"""
    w, H = freqz(h, worN=8192)
    w_norm = w / np.pi
    stopband = np.abs(H[w_norm > 0.35])
    if len(stopband) == 0:
        return 0.0
    return -20.0 * np.log10(np.max(stopband) + 1e-300)

def test_intersample_peak(h):
    """
    Test: alternating +0.9/-0.9 signal should produce
    intersample peak > 0.9 after 4x interpolation.
    Classic ITU-R BS.1770-4 test case.

    IMPORTANT: Use 50 repetitions to ensure the FIR
    has enough samples to settle fully regardless of
    tap count. Empty array guard prevents np.max crash.
    """
    # 50 repetitions = 400 samples — enough for any tap count
    pattern = np.array([0.9, -0.9] * 50, dtype=np.float64)
    # Upsample 4x (insert zeros between samples)
    upsampled = np.zeros(len(pattern) * OVERSAMPLE)
    upsampled[::OVERSAMPLE] = pattern * OVERSAMPLE
    # Apply FIR interpolation filter
    interpolated = lfilter(h, 1.0, upsampled)
    # Skip settling period
    settling = len(h)
    tail = interpolated[settling:]
    if len(tail) == 0:
        return 0.0  # guard against empty array
    return float(np.max(np.abs(tail)))

# Find minimum taps that achieve:
# 1. Stopband attenuation >= 60 dB
# 2. Intersample peak > 0.9
results = []
best = None

for n_taps in range(7, 128, 2):  # odd taps only
    h = design_fir(n_taps)
    attenuation = measure_stopband_attenuation(h)
    peak = test_intersample_peak(h)
    meets = bool(attenuation >= 60.0 and peak > 0.9)
    results.append({
        "n_taps": n_taps,
        "stopband_attenuation_db": round(float(attenuation), 2),
        "intersample_peak": round(float(peak), 6),
        "meets_spec": meets
    })
    if meets and best is None:
        n_phases = OVERSAMPLE
        taps_per_phase = (n_taps + n_phases - 1) // n_phases
        
        phases = []
        flush_count = 0
        pad_count = 0
        
        for k in range(n_phases):
            phase = []
            for i in range(k, n_taps, n_phases):
                coeff = float(h[i]) * float(n_phases)
                if abs(coeff) < 1e-15:
                    coeff = 0.0
                    flush_count += 1
                phase.append(coeff)
            while len(phase) < taps_per_phase:
                phase.append(0.0)
                pad_count += 1
            phases.append(phase)
            
        assert all(len(p) == taps_per_phase for p in phases), "Phase length mismatch!"
        total_macs = n_phases * taps_per_phase
        
        clean_h = [0.0 if abs(float(c)) < 1e-15 else float(c) for c in h.tolist()]
        
        best = {
            "n_taps": n_taps,
            "coefficients": clean_h,
            "stopband_attenuation_db": round(float(attenuation), 2),
            "intersample_peak": round(float(peak), 6),
            "oversample_factor": OVERSAMPLE,
            "cutoff_normalized": 0.25,
            "sample_rate": FS,
            "upsampled_rate": FS_UP,
            "window": "blackmanharris",
            "polyphase": {
                "n_phases": n_phases,
                "taps_per_phase": taps_per_phase,
                "phases": phases,
                "total_macs_per_input_sample": total_macs,
                "flush_to_zero_count": flush_count,
                "zero_pad_count": pad_count,
                "rust_array_type": f"[[f64; {taps_per_phase}]; {n_phases}]"
            }
        }
        print(f"MINIMUM TAPS FOUND: {n_taps}")
        print(f"  Attenuation: {attenuation:.1f} dB")
        print(f"  Intersample peak: {peak:.6f}")
        print(f"  Phase 3 array: {phases[3]}")

print("\nSweep results (first 15):")
for r in results[:15]:
    print(f"  {r['n_taps']:3d} taps: "
          f"{r['stopband_attenuation_db']:6.1f} dB, "
          f"peak={r['intersample_peak']:.4f}, "
          f"{'✅' if r['meets_spec'] else '❌'}")

if best is None:
    print("ERROR: No FIR found that meets spec!")
else:
    output = {
        "minimum_fir": best,
        "sweep": results
    }
    os.makedirs("tests/fixtures", exist_ok=True)
    with open("tests/fixtures/true_peak_fir.json", "w") as f:
        json.dump(output, f, indent=2)
    print("\nWritten: tests/fixtures/true_peak_fir.json")
