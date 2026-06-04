#!/usr/bin/env python3
# tools/oracle/stft_oracle.py
import numpy as np
import scipy.signal as sig
import json

SAMPLE_RATE = 48000
FFT_SIZE    = 2048
HOP_SIZE    = 512

def stft_oracle(signal: np.ndarray) -> dict:
    """Ground truth STFT using SciPy."""
    f, t, Zxx = sig.stft(
        signal,
        fs=SAMPLE_RATE,
        nperseg=FFT_SIZE,
        noverlap=FFT_SIZE - HOP_SIZE,
        window='hann',
        return_onesided=True,
    )
    return {
        "magnitude": np.abs(Zxx).tolist(),
        "phase":     np.angle(Zxx).tolist(),
        "n_frames":  Zxx.shape[1],
        "n_bins":    Zxx.shape[0],
    }

def perfect_reconstruction_test(signal: np.ndarray) -> dict:
    """Verify inverse(forward(x)) == x."""
    f, t, Zxx = sig.stft(signal, fs=SAMPLE_RATE,
        nperseg=FFT_SIZE, noverlap=FFT_SIZE-HOP_SIZE,
        window='hann')
    _, x_rec = sig.istft(Zxx, fs=SAMPLE_RATE,
        nperseg=FFT_SIZE, noverlap=FFT_SIZE-HOP_SIZE,
        window='hann')
    
    min_len = min(len(signal), len(x_rec))
    mse = np.mean((signal[:min_len] - x_rec[:min_len])**2)
    
    return {
        "mse": float(mse),
        "pass": bool(mse < 1e-6),
        "signal_length": len(signal),
        "reconstructed_length": len(x_rec),
    }

if __name__ == "__main__":
    np.random.seed(42)
    # Test 1: sine wave
    t = np.linspace(0, 1, SAMPLE_RATE)
    sine = np.sin(2 * np.pi * 440 * t).astype(np.float32)
    result = perfect_reconstruction_test(sine)
    
    # Test 2: white noise
    noise = np.random.randn(SAMPLE_RATE).astype(np.float32) * 0.1
    result_noise = perfect_reconstruction_test(noise)
    
    output = {
        "sine_440hz":  result,
        "white_noise": result_noise,
        "fft_size":    FFT_SIZE,
        "hop_size":    HOP_SIZE,
    }
    with open("tests/fixtures/stft_oracle.json", "w") as f:
        json.dump(output, f, indent=2)
    print(f"STFT Oracle: sine MSE={result['mse']:.2e} {'PASS' if result['pass'] else 'FAIL'}")
    print(f"STFT Oracle: noise MSE={result_noise['mse']:.2e} {'PASS' if result_noise['pass'] else 'FAIL'}")
