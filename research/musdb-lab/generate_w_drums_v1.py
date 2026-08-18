import os
import numpy as np
import librosa
import struct
import re
import matplotlib.pyplot as plt
import soundfile as sf

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
    return np.abs(D)

def update_W_KL(V, W, H):
    V_hat = np.zeros_like(V)
    for tau in range(W.shape[2]):
        H_shifted = np.pad(H, ((0,0), (tau,0)), mode='constant')[:, :V.shape[1]]
        V_hat += np.dot(W[:, :, tau], H_shifted)
    
    V_hat = np.maximum(V_hat, 1e-12)
    ratio = V / V_hat
    
    W_new = np.zeros_like(W)
    for tau in range(W.shape[2]):
        H_shifted = np.pad(H, ((0,0), (tau,0)), mode='constant')[:, :V.shape[1]]
        num = np.dot(ratio, H_shifted.T)
        den = np.dot(np.ones_like(ratio), H_shifted.T)
        W_new[:, :, tau] = W[:, :, tau] * num / np.maximum(den, 1e-12)
    return W_new

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

def run_nmfd(V, K=3, tau=8, n_iter=30):
    M, N = V.shape
    np.random.seed(42)
    W = np.random.rand(M, K, tau).astype(np.float32)
    H = np.random.rand(K, N).astype(np.float32)
    
    print(f"Running NMFD on V shape {V.shape}...")
    for i in range(n_iter):
        H = update_H_KL(V, W, H)
        W = update_W_KL(V, W, H)
        
        for k in range(K):
            norm = np.sum(W[:, k, :])
            if norm > 0:
                W[:, k, :] /= norm
                H[k, :] *= norm
                
        if i % 5 == 0 or i == n_iter - 1:
            V_hat = np.zeros_like(V)
            for t in range(W.shape[2]):
                H_shifted = np.pad(H, ((0,0), (t,0)), mode='constant')[:, :V.shape[1]]
                V_hat += np.dot(W[:, :, t], H_shifted)
            kl = np.sum(V * np.log(np.maximum(V / np.maximum(V_hat, 1e-12), 1e-12)) - V + V_hat)
            print(f"Iter {i}: KL = {kl:.4f}")
    return W, H

def align_and_normalize(W):
    K = W.shape[1]
    tau_len = W.shape[2]
    W_aligned = np.zeros_like(W)
    
    for k in range(K):
        # find peak tau based on energy
        energy = np.sum(W[:, k, :], axis=0)
        tau_max = np.argmax(energy)
        
        # We want the peak to be at tau = 1 (onset)
        shift = tau_max - 1
        
        if shift > 0:
            # Shift left by `shift` frames
            W_aligned[:, k, :tau_len-shift] = W[:, k, shift:]
        elif shift < 0:
            # Shift right by 1 frame (if peak is at 0)
            W_aligned[:, k, -shift:] = W[:, k, :tau_len+shift]
        else:
            W_aligned[:, k, :] = W[:, k, :]
            
        # Normalize to L1 = 1.0
        norm = np.sum(np.abs(W_aligned[:, k, :]))
        if norm > 0:
            W_aligned[:, k, :] /= norm
            
    return W_aligned

def main():
    mel_matrix = parse_mel_matrix()
    
    test_dir = os.path.join(MUSDB_DIR, "test")
    if not os.path.isdir(test_dir):
        print(f"Error: Could not find {test_dir}")
        return
        
    tracks = [d for d in os.listdir(test_dir) if os.path.isdir(os.path.join(test_dir, d))]
    # Since there are only 20 test tracks and no train, we use all 20 full tracks
    all_mel_specs = []
    
    print(f"Loading {len(tracks)} FULL drum stems...")
    total_frames = 0
    for track in tracks:
        drum_path = os.path.join(test_dir, track, "drums.wav")
        if not os.path.exists(drum_path):
            continue
            
        y, sr = librosa.load(drum_path, sr=48000, mono=True)
        mag_spec = compute_stft(y)
        mel_spec = np.dot(mel_matrix, mag_spec)
        all_mel_specs.append(mel_spec)
        total_frames += mel_spec.shape[1]
        print(f"Processed {track} ({mel_spec.shape[1]} frames)")
        
    if not all_mel_specs:
        print("No drum stems loaded!")
        return
        
    V = np.concatenate(all_mel_specs, axis=1)
    V = np.maximum(V, 1e-12)
    
    W, H = run_nmfd(V, K=3, tau=8, n_iter=40)
    
    W_aligned = align_and_normalize(W)
    
    out_bin = "w_drums_v1.bin"
    with open(out_bin, "wb") as f:
        f.write(W_aligned.flatten().astype(np.float32).tobytes())
    print(f"Saved w_drums_v1.bin (Shape: {W_aligned.shape})")
    
    fig, axes = plt.subplots(1, 3, figsize=(15, 5))
    for k in range(3):
        im = axes[k].imshow(W_aligned[:, k, :], origin='lower', aspect='auto', cmap='viridis', interpolation='nearest')
        axes[k].set_title(f"Component {k} (Peak @ tau=1)")
        axes[k].set_xlabel("Tau (frames)")
        axes[k].set_ylabel("Mel Band")
        fig.colorbar(im, ax=axes[k])
        
    plt.tight_layout()
    plt.savefig("w_drums_v1.png", dpi=150)
    print("Saved plot to w_drums_v1.png")

if __name__ == "__main__":
    main()
