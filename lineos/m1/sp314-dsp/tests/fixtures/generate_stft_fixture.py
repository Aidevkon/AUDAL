import numpy as np
import json, os

# STFT Parameters
FFT_SIZE  = 2048
HOP_SIZE  = 512   # 75% overlap (2048 * 0.25)
FS        = 48000

# 1. Generate PERIODIC Hann window (crucial for Perfect Reconstruction)
# np.hanning is symmetric (divides by N-1). We need periodic (divides by N).
n_arr = np.arange(FFT_SIZE)
hann = (0.5 - 0.5 * np.cos(2 * np.pi * n_arr / FFT_SIZE)).astype(np.float64)

# 2. Verify Perfect Reconstruction condition
# For 75% overlap (4 frames), the sum of squared Hann windows is exactly 1.5
ola_sum = np.zeros(FFT_SIZE * 4, dtype=np.float64)
for k in range(8):  # 8 overlapping frames
    start = k * HOP_SIZE
    ola_sum[start:start + FFT_SIZE] += hann ** 2

# Check middle region (fully overlapped)
mid = FFT_SIZE
ola_check = ola_sum[mid:mid + HOP_SIZE]
ola_min = float(np.min(ola_check))
ola_max = float(np.max(ola_check))
ola_mean = float(np.mean(ola_check))

print("=== OLA SUM VERIFICATION ===")
print(f"OLA sum min:  {ola_min:.10f}")
print(f"OLA sum max:  {ola_max:.10f}")
print(f"OLA sum mean: {ola_mean:.10f}")
# Check against 1.5 (4 * 0.375 = 1.5)
print(f"Perfect OLA sum check: {abs(ola_mean - 1.5) < 1e-10}")

# 3. Generate test signals
n_samples = FFT_SIZE * 4  # 4 frames worth

# Signal A: 1kHz sine
t = np.arange(n_samples) / FS
sine_1k = (0.5 * np.sin(2 * np.pi * 1000 * t)).astype(np.float64)

# Signal B: white noise (fixed seed for determinism)
rng = np.random.default_rng(42)
white_noise = (0.3 * rng.standard_normal(n_samples)).astype(np.float64)

# 4. STFT -> iSTFT round-trip test
def stft_istft(signal, fft_size, hop_size, window):
    """Perfect reconstruction weighted overlap-add STFT/iSTFT"""
    n = len(signal)
    frames = []
    # Analysis
    for i in range(0, n - fft_size + 1, hop_size):
        frame = signal[i:i + fft_size] * window
        spectrum = np.fft.rfft(frame)
        frames.append(spectrum)

    # Synthesis
    output = np.zeros(n, dtype=np.float64)
    window_sum = np.zeros(n, dtype=np.float64)
    for k, spectrum in enumerate(frames):
        frame = np.fft.irfft(spectrum)
        start = k * hop_size
        end = start + fft_size
        if end <= n:
            output[start:end] += frame * window
            window_sum[start:end] += window ** 2

    # Normalize by the OLA sum (which is ~1.5)
    mask = window_sum > 1e-8
    output[mask] /= window_sum[mask]
    return output

# Test round-trip
sine_rt  = stft_istft(sine_1k,    FFT_SIZE, HOP_SIZE, hann)
noise_rt = stft_istft(white_noise, FFT_SIZE, HOP_SIZE, hann)

# Compute reconstruction error (skip edges)
margin = FFT_SIZE
sine_err  = float(np.max(np.abs(sine_1k[margin:-margin] - sine_rt[margin:-margin])))
noise_err = float(np.max(np.abs(white_noise[margin:-margin] - noise_rt[margin:-margin])))

print("\n=== RECONSTRUCTION ERROR ===")
print(f"Sine  error: {sine_err:.2e}")
print(f"Noise error: {noise_err:.2e}")
passes = sine_err < 1e-10 and noise_err < 1e-10
print(f"Perfect reconstruction (< 1e-10): {passes}")

output = {
    "fft_size":  FFT_SIZE,
    "hop_size":  HOP_SIZE,
    "sample_rate": FS,
    "n_bins": FFT_SIZE // 2 + 1,
    "hann_window": hann.tolist(),
    "ola_sum_mean": ola_mean,
    "ola_compensation_factor": round(1.0 / ola_mean, 10),
    "perfect_reconstruction": {
        "sine_1k_error":   sine_err,
        "white_noise_error": noise_err,
        "passes": passes
    }
}

os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/stft_reference.json", "w") as f:
    json.dump(output, f, indent=2)
print("\nWritten: tests/fixtures/stft_reference.json")
