#!/usr/bin/env python3
"""
Creator OS — PCA Spatial Oracle
Ground truth: adaptive M/S via PCA on stereo signal.
Authority: Orthogonal Transforms Spec v1.1 OT-P5
INV-OT-3: deterministic (fixed seed, no randomness)
"""
import numpy as np
import json, hashlib, os

np.random.seed(42)
SAMPLE_RATE = 48000

def pca_spatial(left: np.ndarray, right: np.ndarray) -> dict:
    """
    PCA on stereo signal → adaptive principal components.
    Returns principal axes + correlation + energy split.
    """
    # Stack stereo as 2×N matrix
    X = np.vstack([left, right]).astype(np.float64)  # 2 × N
    
    # Covariance matrix (2×2)
    cov = np.cov(X)  # [[Cll, Clr], [Crl, Crr]]
    
    # Eigendecomposition (deterministic for 2×2)
    eigenvalues, eigenvectors = np.linalg.eigh(cov)
    
    # Sort by descending eigenvalue
    idx = np.argsort(eigenvalues)[::-1]
    eigenvalues  = eigenvalues[idx]
    eigenvectors = eigenvectors[:, idx]
    
    # Principal components
    PC1 = eigenvectors[:, 0]  # dominant axis
    PC2 = eigenvectors[:, 1]  # secondary axis
    
    # Energy split
    total_energy  = eigenvalues.sum()
    pc1_ratio     = float(eigenvalues[0] / total_energy) if total_energy > 1e-10 else 0.5
    
    # Stereo correlation
    ll = float(cov[0, 0])
    rr = float(cov[1, 1])
    lr = float(cov[0, 1])
    denom = np.sqrt(ll * rr)
    correlation = float(lr / denom) if denom > 1e-10 else 0.0
    
    # Adaptive M/S angle (rotation from fixed M/S)
    ms_angle = float(np.arctan2(PC1[1], PC1[0]))
    
    return {
        "pc1":          PC1.tolist(),
        "pc2":          PC2.tolist(),
        "eigenvalues":  eigenvalues.tolist(),
        "pc1_ratio":    pc1_ratio,
        "correlation":  correlation,
        "ms_angle_rad": ms_angle,
        "cov_ll":       ll,
        "cov_rr":       rr,
        "cov_lr":       lr,
    }

# Test signals
t = np.linspace(0, 1, SAMPLE_RATE, endpoint=False).astype(np.float32)

# Signal 1: correlated stereo (mono-like)
mono   = np.sin(2 * np.pi * 440 * t)
left1  = (mono + np.random.randn(SAMPLE_RATE) * 0.05).astype(np.float32)
right1 = (mono + np.random.randn(SAMPLE_RATE) * 0.05).astype(np.float32)

# Signal 2: uncorrelated stereo (wide)
left2  = np.random.randn(SAMPLE_RATE).astype(np.float32) * 0.5
right2 = np.random.randn(SAMPLE_RATE).astype(np.float32) * 0.5

# Signal 3: pure M/S (45° rotation)
mid   = np.sin(2 * np.pi * 220 * t).astype(np.float32)
side  = np.sin(2 * np.pi * 330 * t).astype(np.float32) * 0.3
left3  = (mid + side) / np.sqrt(2)
right3 = (mid - side) / np.sqrt(2)

r1 = pca_spatial(left1, right1)
r2 = pca_spatial(left2, right2)
r3 = pca_spatial(left3, right3)

fixture = {
    "version":         "1.0",
    "correlated":      r1,
    "uncorrelated":    r2,
    "ms_signal":       r3,
    "assertions": {
        "correlated_high_correlation":   r1["correlation"] > 0.8,
        "uncorrelated_low_correlation":  abs(r2["correlation"]) < 0.1,
        "correlated_pc1_ratio_high":     r1["pc1_ratio"] > 0.9,
        "uncorrelated_pc1_ratio_near_half": 0.4 < r2["pc1_ratio"] < 0.6,
    }
}

os.makedirs("lineos/m1/sp314-dsp/tests/fixtures", exist_ok=True)
path = "lineos/m1/sp314-dsp/tests/fixtures/pca_spatial_oracle.json"
with open(path, "w") as f:
    json.dump(fixture, f, indent=2)

with open(path, "rb") as f:
    sha = hashlib.sha256(f.read()).hexdigest()
with open(path.replace(".json", ".lock"), "w") as f:
    f.write(sha)

print(f"Correlated   correlation: {r1['correlation']:.4f}  pc1_ratio: {r1['pc1_ratio']:.4f}")
print(f"Uncorrelated correlation: {r2['correlation']:.4f}  pc1_ratio: {r2['pc1_ratio']:.4f}")
print(f"M/S signal   ms_angle:   {r3['ms_angle_rad']:.4f} rad")
for k, v in fixture["assertions"].items():
    print(f"  {k}: {'PASS' if v else 'FAIL'}")
print(f"SHA-256: {sha[:16]}...")
