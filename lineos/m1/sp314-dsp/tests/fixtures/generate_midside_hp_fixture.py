import math
import json
import os

SAMPLE_RATE = 48000.0
FREQ = 100.0
Q = 0.70710678  # Butterworth — maximally flat

w0     = 2.0 * math.pi * FREQ / SAMPLE_RATE
alpha  = math.sin(w0) / (2.0 * Q)
cos_w0 = math.cos(w0)

# RBJ High-Pass Filter
b0 =  (1.0 + cos_w0) / 2.0
b1 = -(1.0 + cos_w0)
b2 =  (1.0 + cos_w0) / 2.0
a0 =  1.0 + alpha
a1 = -2.0 * cos_w0
a2 =  1.0 - alpha

# Normalize by a0
b0_n = b0 / a0
b1_n = b1 / a0
b2_n = b2 / a0
a1_n = a1 / a0
a2_n = a2 / a0

output = {
    "filter_type": "highpass_butterworth",
    "cutoff_hz": FREQ,
    "q_factor": Q,
    "sample_rate": SAMPLE_RATE,
    "purpose": "Mid-Side EQ HP filter — DC blocking for S channel",
    "coefficients": {
        "b0": b0_n,
        "b1": b1_n,
        "b2": b2_n,
        "a1": a1_n,
        "a2": a2_n
    }
}

os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/midside_hp_biquad.json", "w") as f:
    json.dump(output, f, indent=2)

print("=== BIQUAD HP COEFFICIENTS ===")
print(f"b0: {b0_n:.10f}")
print(f"b1: {b1_n:.10f}")
print(f"b2: {b2_n:.10f}")
print(f"a1: {a1_n:.10f}")
print(f"a2: {a2_n:.10f}")
print("==============================")

# Cross-verify against existing biquad_reference.json
# (Phase 3 fixture has HP at 100Hz/0.707 — values must match)
try:
    with open("tests/fixtures/biquad_reference.json") as f:
        ref = json.load(f)
    for entry in ref["filters"]:
        if entry["type"] == "highpass":
            ref_coeffs = entry["coefficients"]
            delta_b0 = abs(b0_n - ref_coeffs["b0"])
            delta_a1 = abs(a1_n - ref_coeffs["a1"])
            print(f"\nCross-check vs Phase 3 fixture:")
            print(f"  b0 delta: {delta_b0:.2e}")
            print(f"  a1 delta: {delta_a1:.2e}")
            if delta_b0 < 1e-8 and delta_a1 < 1e-8:
                print("  ✅ MATCH — coefficients consistent")
            else:
                print("  ⚠️  MISMATCH — investigate")
except FileNotFoundError:
    print("(biquad_reference.json not found — skip cross-check)")
