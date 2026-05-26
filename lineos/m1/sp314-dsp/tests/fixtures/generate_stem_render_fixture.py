import numpy as np
from scipy.ndimage import median_filter
import json, os

FFT_SIZE = 2048
HOP_SIZE = 512
FS       = 48000

# Periodic Hann window
n_arr = np.arange(FFT_SIZE)
hann  = (0.5 - 0.5 * np.cos(2 * np.pi * n_arr / FFT_SIZE)).astype(np.float64)

# Generate test signal: sine + impulse
n_samples = FFT_SIZE * 8
t         = np.arange(n_samples) / FS
signal    = (0.5 * np.sin(2 * np.pi * 440 * t)).astype(np.float64)

# Add impulse at frame 4 (center of the window)
impulse_pos = 4 * HOP_SIZE + FFT_SIZE // 2
signal[impulse_pos] += 0.8

# STFT
def stft(sig, fft_size, hop_size, window):
    frames = []
    phases = []
    for i in range(0, len(sig) - fft_size + 1, hop_size):
        frame = sig[i:i+fft_size] * window
        S     = np.fft.rfft(frame)
        frames.append(np.abs(S))
        phases.append(np.angle(S))
    return np.array(frames), np.array(phases)

# iSTFT with WOLA
def istft(mag, phase, fft_size, hop_size, window, n):
    output     = np.zeros(n)
    window_sum = np.zeros(n)
    for k in range(len(mag)):
        S     = mag[k] * np.exp(1j * phase[k])
        # n=fft_size is critical to guarantee exactly 2048 samples from irfft
        frame = np.fft.irfft(S, n=fft_size) 
        start = k * hop_size
        end   = min(start + fft_size, n)
        length = end - start
        
        # iSTFT normalization factor inside WOLA (frame / FFT_SIZE) 
        # Numpy's irfft handles its own normalization, but we scale by window
        output[start:end]     += frame[:length] * window[:length]
        window_sum[start:end] += window[:length] ** 2
        
    mask = window_sum > 1e-8
    output[mask] /= window_sum[mask]
    return output

magnitudes, phases = stft(signal, FFT_SIZE, HOP_SIZE, hann)
n_frames, n_bins   = magnitudes.shape

# HPSS Median Filters
L = 17
mag_harm = np.zeros_like(magnitudes)
mag_perc = np.zeros_like(magnitudes)
for b in range(n_bins):
    mag_harm[:, b] = median_filter(magnitudes[:, b], size=L, mode='nearest')
for t in range(n_frames):
    mag_perc[t, :] = median_filter(magnitudes[t, :], size=L, mode='nearest')

# Masks (Matching Rust 50/50 Digital Silence Fix)
mask_h = np.zeros_like(magnitudes)
mask_p = np.zeros_like(magnitudes)

for t in range(n_frames):
    for b in range(n_bins):
        h = float(mag_harm[t, b])
        p = float(mag_perc[t, b])
        d = h + p
        if d < 1e-8:
            mask_h[t, b] = 0.5
            mask_p[t, b] = 0.5
        else:
            mask_h[t, b] = h / d
            mask_p[t, b] = p / d

# Apply Masks
mag_stem_h = magnitudes * mask_h
mag_stem_p = magnitudes * mask_p

# Render Stems
stem_h = istft(mag_stem_h, phases, FFT_SIZE, HOP_SIZE, hann, n_samples)
stem_p = istft(mag_stem_p, phases, FFT_SIZE, HOP_SIZE, hann, n_samples)

# Probe points to verify in Rust Contract Test
probe_h_steady  = float(stem_h[1000])        # Should be sine wave
probe_p_impulse = float(stem_p[impulse_pos]) # Should contain the impulse energy
probe_h_impulse = float(stem_h[impulse_pos]) # Should NOT have the impulse

print("=== STEM RENDER ANALYSIS ===")
print(f"Original Impulse + Sine at pos {impulse_pos}: {signal[impulse_pos]:.4f}")
print(f"Harmonic Stem at impulse pos: {probe_h_impulse:.4f} (expect sine value, ~0.5 max)")
print(f"Percussive Stem at impulse pos: {probe_p_impulse:.4f} (expect high energy, ~0.8)")

output = {
    "fft_size": FFT_SIZE,
    "hop_size": HOP_SIZE,
    "sample_rate": FS,
    "n_samples": n_samples,
    "impulse_pos": int(impulse_pos),
    "probe_h_steady": probe_h_steady,
    "probe_p_impulse": probe_p_impulse,
    "probe_h_impulse": probe_h_impulse
}

os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/stem_render_reference.json", "w") as f:
    json.dump(output, f, indent=2)
print("Written: tests/fixtures/stem_render_reference.json")
