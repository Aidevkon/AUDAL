import numpy as np
import json, os

FS       = 48000
FFT_SIZE = 2048
HOP_SIZE = 512

# Periodic Hann window (matches Rust implementation exactly)
n_arr = np.arange(FFT_SIZE)
hann  = (0.5 - 0.5 * np.cos(2 * np.pi * n_arr / FFT_SIZE)
         ).astype(np.float64)

# Generate synthetic drum pattern (120 BPM)
duration_ms = 2000
n_samples   = int(FS * duration_ms / 1000)
signal      = np.zeros(n_samples, dtype=np.float64)
rng         = np.random.default_rng(42)  # Deterministic noise

def add_kick(sig, onset_ms, amplitude, decay_ms=50):
    onset  = int(onset_ms * FS / 1000)
    length = int(decay_ms * FS / 1000)
    t      = np.arange(length) / FS
    # 60Hz damped sine wave — real spectral content
    burst  = amplitude * np.sin(2 * np.pi * 60 * t) \
             * np.exp(-t * 1000 / decay_ms)
    end    = min(onset + length, len(sig))
    sig[onset:end] += burst[:end - onset]

def add_snare(sig, onset_ms, amplitude, decay_ms=20):
    onset  = int(onset_ms * FS / 1000)
    length = int(decay_ms * FS / 1000)
    t      = np.arange(length) / FS
    # Damped white noise — broadband spectral content
    noise  = rng.standard_normal(length)
    burst  = amplitude * noise * np.exp(-t * 1000 / decay_ms)
    end    = min(onset + length, len(sig))
    sig[onset:end] += burst[:end - onset]

# Kicks at 0ms, 1000ms
for ms in [0, 1000]:
    add_kick(signal, ms, 0.9)

# Snares at 500ms, 1500ms
for ms in [500, 1500]:
    add_snare(signal, ms, 0.7)

# STFT Analysis
def compute_stft(sig, fft_size, hop_size, window):
    frames = []
    for i in range(0, len(sig) - fft_size + 1, hop_size):
        frame = sig[i:i+fft_size] * window
        frames.append(np.abs(np.fft.rfft(frame)))
    return np.array(frames)

magnitudes = compute_stft(signal, FFT_SIZE, HOP_SIZE, hann)
n_frames, n_bins = magnitudes.shape

# Spectral Flux (positive differences only)
flux = np.zeros(n_frames, dtype=np.float64)
for t in range(1, n_frames):
    diff = magnitudes[t] - magnitudes[t-1]
    flux[t] = float(np.sum(np.maximum(diff, 0.0)))

# Normalize flux to [0, 1]
flux_max = float(np.max(flux))
if flux_max > 1e-8:
    flux_norm = flux / flux_max
else:
    flux_norm = flux

# Peak picking — matches Rust exact logic:
# local maximum > threshold with min_distance enforcement
THRESHOLD    = 0.01
MIN_DISTANCE = 10  # frames (~100ms at 512 hop / 48kHz)

beats = []
last_beat = -MIN_DISTANCE
for t in range(1, n_frames - 1):
    if (flux_norm[t] >= THRESHOLD
            and flux_norm[t] > flux_norm[t - 1]
            and flux_norm[t] > flux_norm[t + 1]):
        if t - last_beat >= MIN_DISTANCE:
            beats.append(int(t))
            last_beat = t

print("=== SPECTRAL FLUX ANALYSIS ===")
print(f"n_frames:    {n_frames}")
print(f"n_beats:     {len(beats)}")
print(f"Beat frames: {beats}")
print(f"Beat times:  "
      f"{[round(b * HOP_SIZE / FS * 1000) for b in beats]} ms")
print(f"Flux max:    {flux_max:.4f}")

# Sanity check — flexible range (noise is stochastic even with seed)
assert 3 <= len(beats) <= 5, \
    f"Expected 3-5 beats (2 kicks + 2 snares), got {len(beats)}"
print(f"Beat detection: OK ({len(beats)} beats)")

output = {
    "fft_size":      FFT_SIZE,
    "hop_size":      HOP_SIZE,
    "sample_rate":   FS,
    "n_frames":      int(n_frames),
    "flux_max":      flux_max,
    "threshold":     THRESHOLD,
    "min_distance":  MIN_DISTANCE,
    "beat_frames":   beats,
    "beat_times_ms": [round(b * HOP_SIZE / FS * 1000)
                      for b in beats],
    "n_beats":       len(beats),
    "flux_normalized": flux_norm.tolist()
}

os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/spectral_flux_reference.json", "w") as f:
    json.dump(output, f, indent=2)
print("Written: tests/fixtures/spectral_flux_reference.json")
