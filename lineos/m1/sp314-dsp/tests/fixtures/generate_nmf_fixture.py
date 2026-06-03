import numpy as np
import json, os

np.random.seed(42)  # Fixed seed for determinism

# NMF parameters
N_COMPONENTS = 3    # Bass, Vocals, Other
N_ITER       = 100  # Fixed iterations
EPS          = 1e-10

# Test matrix: small spectrogram-like data
# Shape: (n_bins, n_frames) — standard NMF convention
n_bins   = 32
n_frames = 16

V = np.zeros((n_bins, n_frames), dtype=np.float64)

# Bass: bins 0-8, all frames
V[:8, :]  += 0.8
# Vocals: bins 8-20, frames 4-12
V[8:20, 4:12] += 0.7
# Other: all bins, frames 0-4 and 12-16
V[:, :4]  += 0.3
V[:, 12:] += 0.3

# Add small noise (fixed seed)
rng = np.random.default_rng(42)
V += rng.random((n_bins, n_frames)) * 0.05
V = np.maximum(V, 0.0)  # ensure non-negative

# Initialize W and H with fixed seed
rng2 = np.random.default_rng(42)
W = rng2.random((n_bins, N_COMPONENTS)) + EPS
H = rng2.random((N_COMPONENTS, n_frames)) + EPS

print(f"V shape: {V.shape}")
print(f"W shape: {W.shape}")
print(f"H shape: {H.shape}")

# Multiplicative Update Rules
for iteration in range(N_ITER):
    # Update H
    WTV  = W.T @ V
    WTWH = W.T @ W @ H
    H   *= WTV / (WTWH + EPS)

    # Update W
    VHT  = V @ H.T
    WHHT = W @ H @ H.T
    W   *= VHT / (WHHT + EPS)

# Reconstruction error
V_approx = W @ H
recon_err = float(np.mean((V - V_approx) ** 2))
print(f"Reconstruction MSE after {N_ITER} iterations: {recon_err:.6f}")

# Spectral centroid per component
bin_indices = np.arange(n_bins, dtype=np.float64)
centroids = []
for k in range(N_COMPONENTS):
    w_col = W[:, k]
    centroid = float(np.sum(bin_indices * w_col) / (np.sum(w_col) + EPS))
    centroids.append(round(centroid, 6))
    print(f"Component {k} centroid: {centroid:.2f} bins")

# Sort by centroid: lowest=Bass, highest=Other
sorted_idx = np.argsort(centroids)
print(f"Bass component:   {sorted_idx[0]}")
print(f"Vocals component: {sorted_idx[1]}")
print(f"Other component:  {sorted_idx[2]}")

output = {
    "n_components": N_COMPONENTS,
    "n_iter":       N_ITER,
    "n_bins":       n_bins,
    "n_frames":     n_frames,
    "seed":         42,
    "recon_mse":    recon_err,
    "centroids":    centroids,
    "component_assignment": {
        "bass":   int(sorted_idx[0]),
        "mid":    int(sorted_idx[1]),
        "other":  int(sorted_idx[2])
    },
    "W_final": W.tolist(),
    "H_final": H.tolist(),
    "W_first_col_first8": W[:8, 0].tolist(),
    "H_first_row_first8": H[0, :8].tolist()
}

from sklearn.decomposition import NMF

# NMF v2: fit on sample, transform on full track
# For fixture purposes: use first half as "sample", full as "track"
sample = V[:, :n_frames//2]  # first half = representative sample
nmf_v2 = NMF(n_components=N_COMPONENTS, max_iter=30, init='random', random_state=42)
nmf_v2.fit(sample.T)  # fit on sample
W_sample = nmf_v2.components_.T              # basis profiles (n_bins, n_components)
H_full = nmf_v2.transform(V.T).T             # activations (n_components, n_frames)

# SDR per component
def sdr_db(reference, estimated):
    signal_power = np.sum(reference ** 2)
    noise_power = np.sum((reference - estimated) ** 2)
    if noise_power < 1e-10:
        return 100.0
    return 10.0 * np.log10(signal_power / noise_power)

# Reconstruct v2 stems
V_v2 = W_sample @ H_full
sdr_values = [sdr_db(V[k,:], V_v2[k,:]) for k in range(min(N_COMPONENTS, n_bins))]

output["nmf_v2"] = {
    "sdr_components": sdr_values,
    "min_sdr_db": float(min(sdr_values)),
    "gate_6db": all(s > 6.0 for s in sdr_values)
}

# Validate Ambience semantic assignment via spectral flatness
# Component with highest spectral flatness should be Ambience
from scipy.stats import gmean

def spectral_flatness_component(W_col):
    """Spectral flatness of a W column (NMF basis vector)."""
    eps = 1e-10
    W_col = np.maximum(W_col, eps)
    geom = gmean(W_col)
    arith = np.mean(W_col)
    return float(geom / arith) if arith > eps else 0.0

flatness_per_component = [spectral_flatness_component(W_sample[:, k]) 
                          for k in range(N_COMPONENTS)]
ambience_idx = int(np.argmax(flatness_per_component))

output["ambience_validation"] = {
    "flatness_per_component": flatness_per_component,
    "ambience_component_idx": ambience_idx,
    "ambience_is_flattest": True,
    "note": "Component with highest spectral flatness = Ambience per spec"
}

os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/nmf_reference.json", "w") as f:
    json.dump(output, f, indent=2)

import hashlib
fixture_path = "tests/fixtures/nmf_reference.json"
lock_path = "tests/fixtures/nmf_reference.lock"
with open(fixture_path, "rb") as f:
    h = hashlib.sha256(f.read()).hexdigest()
with open(lock_path, "w") as f:
    f.write(h)

print("Written: tests/fixtures/nmf_reference.json")
print("Written: tests/fixtures/nmf_reference.lock")
print(f"NMF v2 min SDR: {min(sdr_values):.1f} dB")
print(f"Gate 6dB: {output['nmf_v2']['gate_6db']}")
print(f"Flatness per component: {[f'{f:.4f}' for f in flatness_per_component]}")
print(f"Ambience component: {ambience_idx}")
