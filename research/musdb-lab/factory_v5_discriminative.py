import os
import glob
import numpy as np
import librosa
import soundfile as sf
import matplotlib.pyplot as plt
import struct
import re
from scipy.signal import butter, sosfiltfilt

MUSDB_DIR = "/home/aidevcon/Downloads/DATASET/musdb18hq"

def parse_mel_matrix():
    rust_path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/src/analysis/mel_128.rs"
    mel_bands = 128
    n_bins = 1025
    
    matrix = np.zeros((mel_bands, n_bins), dtype=np.float32)
    
    with open(rust_path, 'r') as f:
        content = f.read()
        
    start_idx = content.find("pub const MEL_128_MATRIX")
    end_idx = content.find("pub const MEL_128_INVERSE", start_idx)
    matrix_str = content[start_idx:end_idx]
    
    hex_vals = re.findall(r'f32::from_bits\((0x[0-9a-fA-F]+)\)', matrix_str)
    
    for i, hex_str in enumerate(hex_vals):
        band = i // n_bins
        bin_idx = i % n_bins
        int_val = int(hex_str, 16)
        float_val = struct.unpack('>f', struct.pack('>I', int_val))[0]
        matrix[band, bin_idx] = float_val
        
    return matrix

def compute_stft(y, sr=48000, n_fft=2048, hop_length=512):
    window = 0.5 - 0.5 * np.cos(2.0 * np.pi * np.arange(n_fft) / n_fft)
    D = librosa.stft(y, n_fft=n_fft, hop_length=hop_length, window=window, center=True)
    mag = np.abs(D)
    return mag

def update_H_KL(V, W, H):
    V_hat = np.zeros_like(V)
    for tau in range(W.shape[2]):
        H_shifted = np.pad(H, ((0,0), (tau,0)), mode='constant')[:, :V.shape[1]]
        V_hat += np.dot(W[:, :, tau], H_shifted)
        
    V_hat = np.maximum(V_hat, 1e-12)
    ratio = V / V_hat
    
    num = np.zeros_like(H)
    den = np.zeros_like(H)
    
    for tau in range(W.shape[2]):
        if tau == 0:
            ratio_shifted = ratio
            ones_shifted = np.ones_like(ratio)
        else:
            ratio_shifted = np.pad(ratio[:, tau:], ((0,0), (0,tau)), mode='constant')
            ones_shifted = np.pad(np.ones_like(ratio[:, tau:]), ((0,0), (0,tau)), mode='constant')
            
        num += np.dot(W[:, :, tau].T, ratio_shifted)
        den += np.dot(W[:, :, tau].T, ones_shifted)
        
    H_new = H * num / np.maximum(den, 1e-12)
    return H_new

def update_W_KL_partial(V, W, H, num_frozen=0):
    V_hat = np.zeros_like(V)
    for tau in range(W.shape[2]):
        H_shifted = np.pad(H, ((0,0), (tau,0)), mode='constant')[:, :V.shape[1]]
        V_hat += np.dot(W[:, :, tau], H_shifted)
    
    V_hat = np.maximum(V_hat, 1e-12)
    ratio = V / V_hat
    
    W_new = np.zeros_like(W)
    # Only update the free components
    for tau in range(W.shape[2]):
        H_shifted = np.pad(H, ((0,0), (tau,0)), mode='constant')[:, :V.shape[1]]
        num = np.dot(ratio, H_shifted.T)
        den = np.dot(np.ones_like(ratio), H_shifted.T)
        W_updated = W[:, :, tau] * num / np.maximum(den, 1e-12)
        
        # Keep frozen parts intact
        W_new[:, :num_frozen, tau] = W[:, :num_frozen, tau]
        W_new[:, num_frozen:, tau] = W_updated[:, num_frozen:]
        
    return W_new

def run_nmfd_partial(V, K=2, tau=8, n_iter=30, W_init=None, num_frozen=0):
    M, N = V.shape
    np.random.seed(42)
    if W_init is not None:
        W = W_init.copy()
    else:
        W = np.random.rand(M, K, tau).astype(np.float32)
    H = np.random.rand(K, N).astype(np.float32)
    
    print(f"Running NMFD on V shape {V.shape} with K={K} (frozen={num_frozen})...")
    for i in range(n_iter):
        H = update_H_KL(V, W, H)
        W = update_W_KL_partial(V, W, H, num_frozen=num_frozen)
        
        for k in range(num_frozen, K):
            norm = np.sum(W[:, k, :])
            if norm > 0:
                W[:, k, :] /= norm
                H[k, :] *= norm
                
        if i % 10 == 0 or i == n_iter - 1:
            V_hat = np.zeros_like(V)
            for t in range(W.shape[2]):
                H_shifted = np.pad(H, ((0,0), (t,0)), mode='constant')[:, :V.shape[1]]
                V_hat += np.dot(W[:, :, t], H_shifted)
            kl = np.sum(V * np.log(np.maximum(V / np.maximum(V_hat, 1e-12), 1e-12)) - V + V_hat)
            print(f"Iter {i}: KL = {kl:.4f}")
            
    return W, H

def get_frames(track_dir, stem_name, mel_matrix, highpass_200=False):
    path = os.path.join(track_dir, f"{stem_name}.wav")
    if not os.path.exists(path):
        return None
    y, sr = librosa.load(path, sr=48000, mono=True)
    if len(y) > sr * 10:
        y = y[sr*5:sr*15]
        
    if highpass_200:
        sos = butter(8, 200, btype='highpass', fs=sr, output='sos')
        y = sosfiltfilt(sos, y).astype(np.float32)
        
    mag = compute_stft(y)
    mel = np.dot(mel_matrix, mag)
    return mel

def main():
    print("Parsing MEL matrix...")
    mel_matrix = parse_mel_matrix()
    
    test_dir = os.path.join(MUSDB_DIR, "test")
    tracks = [d for d in os.listdir(test_dir) if os.path.isdir(os.path.join(test_dir, d))]
    tracks = tracks[:20]
    
    # -----------------------------
    # STEP 1: W_bass_negative
    # -----------------------------
    print("\n--- STEP 1: Training W_bass_negative ---")
    all_bass = []
    for t in tracks:
        b = get_frames(os.path.join(test_dir, t), "bass", mel_matrix, highpass_200=False)
        if b is not None:
            all_bass.append(b)
            
    V_bass_full = np.concatenate(all_bass, axis=1)
    # Silence cleaning
    e_bass = np.sum(V_bass_full, axis=0)
    V_bass = V_bass_full[:, e_bass > np.mean(e_bass)*0.05]
    V_bass = np.maximum(V_bass, 1e-12)
    
    print(f"Bass frames (cleaned): {V_bass.shape[1]}")
    W_bass, _ = run_nmfd_partial(V_bass, K=2, tau=8, n_iter=30, num_frozen=0)
    
    # Normalize W_bass just in case (run_nmfd_partial normalizes free components anyway)
    for k in range(2):
        W_bass[:, k, :] /= np.sum(W_bass[:, k, :])
        
    # -----------------------------
    # STEP 2: DISCRIMINATIVE SUNG TRAINING
    # -----------------------------
    print("\n--- STEP 2: Discriminative Sung Training ---")
    all_voice = []
    for t in tracks:
        v = get_frames(os.path.join(test_dir, t), "vocals", mel_matrix, highpass_200=True)
        if v is not None:
            all_voice.append(v)
            
    V_voice_full = np.concatenate(all_voice, axis=1)
    e_voice = np.sum(V_voice_full, axis=0)
    V_voice = V_voice_full[:, e_voice > np.mean(e_voice)*0.05]
    V_voice = np.maximum(V_voice, 1e-12)
    
    print(f"Voice frames (cleaned): {V_voice.shape[1]}")
    
    V_mix = np.concatenate((V_voice, V_bass), axis=1)
    print(f"V_mix frames: {V_mix.shape[1]}")
    
    # Init W with W_bass as first 2 components, random for the 3 sung
    W_init = np.random.rand(128, 5, 8).astype(np.float32)
    W_init[:, :2, :] = W_bass
    
    W_mix, H_mix = run_nmfd_partial(V_mix, K=5, tau=8, n_iter=30, W_init=W_init, num_frozen=2)
    
    W_sung = W_mix[:, 2:, :] # The 3 free components
    
    # -----------------------------
    # STEP 3: QUICK SELF-TEST
    # -----------------------------
    print("\n--- STEP 3: Quick Self-Test (Selectivity) ---")
    # Take 500 frames of voice and 500 frames of bass from the cleaned matrices
    test_frames = 500
    V_test_v = V_voice[:, :test_frames]
    V_test_b = V_bass[:, :test_frames]
    
    # Fit H for W_sung on Voice
    H_test_v = np.random.rand(3, test_frames).astype(np.float32)
    for _ in range(15):
        H_test_v = update_H_KL(V_test_v, W_sung, H_test_v)
    act_voice = np.sum(H_test_v)
    
    # Fit H for W_sung on Bass
    H_test_b = np.random.rand(3, test_frames).astype(np.float32)
    for _ in range(15):
        H_test_b = update_H_KL(V_test_b, W_sung, H_test_b)
    act_bass = np.sum(H_test_b)
    
    print(f"Total activation on Voice ({test_frames} frames): {act_voice:.4f}")
    print(f"Total activation on Bass  ({test_frames} frames): {act_bass:.4f}")
    
    ratio = act_voice / max(act_bass, 1e-12)
    print(f"Selectivity Ratio (Voice:Bass) = {ratio:.2f}:1")
    
    if ratio < 3.0:
        print("FAIL: Selectivity < 3:1. Aborting delivery.")
        # We can still plot for debug
    else:
        print("PASS: Selectivity >= 3:1. Delivering w_sung_v5.bin.")
        out_bin = "w_sung_v5.bin"
        with open(out_bin, "wb") as f:
            f.write(W_sung.flatten().astype(np.float32).tobytes())
            
    # Check bin stats
    for k in range(3):
        mass = np.sum(W_sung[:, k, :])
        mel_0_13 = np.sum(W_sung[:14, k, :]) / max(mass, 1e-12)
        print(f"Component {k}: Total Mass = {mass:.4f}, Mel 0-13 = {mel_0_13*100:.2f}%")
        
    # Plot
    fig, axes = plt.subplots(1, 3, figsize=(15, 5))
    for k in range(3):
        template = W_sung[:, k, :]
        im = axes[k].imshow(template, origin='lower', aspect='auto', cmap='viridis', interpolation='nearest')
        axes[k].set_title(f"Discriminative Sung C{k}")
        axes[k].set_xlabel("Tau (frames)")
        axes[k].set_ylabel("Mel Band")
        fig.colorbar(im, ax=axes[k])
        
    plt.tight_layout()
    plt.savefig("w_sung_v5.png", dpi=150)
    print("Saved plot to w_sung_v5.png")

if __name__ == "__main__":
    main()
