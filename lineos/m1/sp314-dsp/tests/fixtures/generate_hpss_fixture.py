import numpy as np
from scipy.ndimage import median_filter
import json, os

FFT_SIZE = 2048
HOP_SIZE = 512
FS       = 48000

# Median filter lengths (must be odd)
L_HARM = 17   # horizontal (time frames) — harmonic
L_PERC = 17   # vertical (frequency bins) — percussive

# Generate test spectrogram:
# Mix of harmonic (sine) + percussive (impulse) content
n_frames  = 64
n_bins    = FFT_SIZE // 2 + 1  # 1025
mag = np.zeros((n_frames, n_bins), dtype=np.float64)

# Harmonic: sustained energy at fixed frequency bins
for bin_idx in [50, 100, 150, 200]:
    mag[:, bin_idx] = 1.0

# Percussive: broadband energy at fixed frames
for frame_idx in [10, 30, 50]:
    mag[frame_idx, :] = 1.0

# Add small noise floor
rng = np.random.default_rng(42)
mag += rng.random((n_frames, n_bins)) * 0.05

# Median filters (Edge Clamping matching Rust logic)
# HARMONIC: filter along time axis (frames) -> axis 0
mag_harm = np.zeros_like(mag)
for b in range(n_bins):
    mag_harm[:, b] = median_filter(mag[:, b], size=L_HARM, mode='nearest')

# PERCUSSIVE: filter along frequency axis (bins) -> axis 1
mag_perc = np.zeros_like(mag)
for t in range(n_frames):
    mag_perc[t, :] = median_filter(mag[t, :], size=L_PERC, mode='nearest')

# Wiener masks (soft masks)
eps = 1e-8
denom = mag_harm + mag_perc + eps
mask_h = mag_harm / denom
mask_p = mag_perc / denom

# Verify: masks sum to 1.0 everywhere
mask_sum = mask_h + mask_p
assert np.allclose(mask_sum, 1.0, atol=1e-6), "Masks don't sum to 1!"

# Verify harmonic mask captures sustained content
harm_at_sustained = float(np.mean(mask_h[:, 50]))
assert harm_at_sustained > 0.7, f"Harmonic mask too weak: {harm_at_sustained}"

# Verify percussive mask captures transient content
perc_at_frame10 = float(np.mean(mask_p[10, :]))
assert perc_at_frame10 > 0.7, f"Percussive mask too weak: {perc_at_frame10}"

# Save reference values
output = {
    "l_harm": L_HARM,
    "l_perc": L_PERC,
    "n_frames_test": n_frames,
    "n_bins_test": n_bins,
    "harm_mask_at_bin50_mean": round(harm_at_sustained, 8),
    "perc_mask_at_frame10_mean": round(perc_at_frame10, 8),
    "mask_sum_check": True,
    "sample_masks": {
        "mask_h_row0": mask_h[0, :8].tolist(),
        "mask_p_row0": mask_p[0, :8].tolist(),
        "mask_h_frame10": mask_h[10, :8].tolist(),
        "mask_p_frame10": mask_p[10, :8].tolist()
    }
}

os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/hpss_reference.json", "w") as f:
    json.dump(output, f, indent=2)
print("Written: tests/fixtures/hpss_reference.json")
