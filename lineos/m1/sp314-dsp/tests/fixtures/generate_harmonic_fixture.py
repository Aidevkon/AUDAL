import numpy as np
from scipy.optimize import minimize_scalar
from scipy.signal.windows import blackmanharris
import json, os

pad_db = -6.0
pad_linear = 10.0 ** (pad_db / 20.0)  # dynamic — no hardcoding

def compute_thd(drive, amplitude, sample_rate=48000, duration=1.0):
    """
    THD (Fundamental) — NOT THD+N.
    THD% = sqrt(sum(H2..H10)^2) / H1 * 100
    """
    n = int(sample_rate * duration)
    t = np.arange(n) / sample_rate
    x = amplitude * np.sin(2 * np.pi * 1000 * t)
    y = np.tanh(drive * x)
    window = blackmanharris(n)
    Y = np.abs(np.fft.rfft(y * window))
    def bin_at(f):
        return int(np.round(f * n / sample_rate))
    h1 = Y[bin_at(1000)]
    harmonics = [Y[bin_at(1000 * k)] for k in range(2, 11)]
    thd = np.sqrt(sum(h**2 for h in harmonics)) / h1 * 100
    return thd

drive_values = [0.1, 0.5, 1.0, 2.0, 5.0, 10.0]
per_drive = []

for drive in drive_values:
    ref_thd = compute_thd(drive, amplitude=1.0)
    result = minimize_scalar(
        lambda K: (compute_thd(K * drive, pad_linear) - ref_thd)**2,
        bounds=(0.5, 5.0), method='bounded'
    )
    K = result.x
    verify_thd = compute_thd(K * drive, pad_linear)
    error = abs(verify_thd - ref_thd)
    per_drive.append({
        "drive": drive,
        "reference_thd_percent": round(ref_thd, 6),
        "K_harmonic": round(K, 8),
        "verification_thd_percent": round(verify_thd, 6),
        "error_percent": round(error, 8)
    })

global_result = minimize_scalar(
    lambda K: np.mean([
        (compute_thd(K * d, pad_linear) - compute_thd(d, 1.0))**2
        for d in drive_values
    ]),
    bounds=(0.5, 5.0), method='bounded'
)
global_K = global_result.x
global_error = np.mean([
    abs(compute_thd(global_K * d, pad_linear) - compute_thd(d, 1.0))
    for d in drive_values
])

output = {
    "pad_db": pad_db,
    "pad_linear": float(pad_linear),
    "per_drive": per_drive,
    "global_K_harmonic": round(float(global_K), 8),
    "global_error_percent": round(float(global_error), 8),
    "method": "THD_fundamental_blackman_harris"
}
os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/k_harmonic_results.json", "w") as f:
    json.dump(output, f, indent=2)
print(json.dumps(output, indent=2))
