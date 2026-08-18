import os
import glob
import wave
import numpy as np
import librosa
import soundfile as sf
import matplotlib.pyplot as plt
import struct
import re

MUSDB_DIR = "/home/aidevcon/Downloads/DATASET/musdb18hq"

def do_recon():
    print("--- MUSDB RECON ---")
    splits = ["train", "test"]
    total_tracks = 0
    
    for split in splits:
        split_dir = os.path.join(MUSDB_DIR, split)
        if not os.path.isdir(split_dir):
            print(f"Split {split} not found at {split_dir}")
            continue
            
        tracks = [d for d in os.listdir(split_dir) if os.path.isdir(os.path.join(split_dir, d))]
        print(f"Split {split}: {len(tracks)} tracks")
        total_tracks += len(tracks)
        
        if len(tracks) > 0:
            track_path = os.path.join(split_dir, tracks[0])
            stems = [f for f in os.listdir(track_path) if f.endswith('.wav')]
            print(f"  Stems in {tracks[0]}: {stems}")
            
            # Check properties of the first stem
            stem_path = os.path.join(track_path, stems[0])
            info = sf.info(stem_path)
            print(f"  Sample Rate: {info.samplerate} Hz")
            print(f"  Channels: {info.channels}")
            print(f"  Duration: {info.duration:.2f} seconds")
            
    print(f"Total Tracks: {total_tracks}")
    print("-------------------\n")

def parse_mel_matrix():
    rust_path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/src/analysis/mel_128.rs"
    mel_bands = 128
    n_bins = 1025
    
    matrix = np.zeros((mel_bands, n_bins), dtype=np.float32)
    
    with open(rust_path, 'r') as f:
        content = f.read()
        
    # Extract MEL_128_MATRIX
    start_idx = content.find("pub const MEL_128_MATRIX")
    end_idx = content.find("pub const MEL_128_INVERSE", start_idx)
    matrix_str = content[start_idx:end_idx]
    
    # Extract f32::from_bits(0x...)
    hex_vals = re.findall(r'f32::from_bits\((0x[0-9a-fA-F]+)\)', matrix_str)
    
    if len(hex_vals) != mel_bands * n_bins:
        print(f"Warning: Expected {mel_bands * n_bins} values, found {len(hex_vals)}")
        
    for i, hex_str in enumerate(hex_vals):
        band = i // n_bins
        bin_idx = i % n_bins
        # Convert hex to float32
        int_val = int(hex_str, 16)
        float_val = struct.unpack('>f', struct.pack('>I', int_val))[0]
        matrix[band, bin_idx] = float_val
        
    return matrix

def compute_stft(y, sr=48000, n_fft=2048, hop_length=512):
    # Rust uses periodic Hann: w[n] = 0.5 - 0.5 * cos(2π * n / FFT_SIZE)
    window = 0.5 - 0.5 * np.cos(2.0 * np.pi * np.arange(n_fft) / n_fft)
    
    D = librosa.stft(y, n_fft=n_fft, hop_length=hop_length, window=window, center=True)
    mag = np.abs(D)
    return mag

def update_W_KL(V, W, H):
    # V_hat = sum_t W[:,:,t] * H_shifted(t)
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
        # shift ratio left by tau
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
        
        # Normalize W over Mel and Tau for each K
        for k in range(K):
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

def main():
    do_recon()
    
    print("Parsing MEL matrix from Rust source...")
    mel_matrix = parse_mel_matrix()
    print(f"MEL matrix shape: {mel_matrix.shape}")
    
    # Load 20 drum stems from MUSDB test split (since we don't have train)
    test_dir = os.path.join(MUSDB_DIR, "test")
    if not os.path.isdir(test_dir):
        print(f"Error: Could not find {test_dir}")
        return
        
    tracks = [d for d in os.listdir(test_dir) if os.path.isdir(os.path.join(test_dir, d))]
    tracks = tracks[:20]
    
    all_mel_specs = []
    
    print(f"Loading {len(tracks)} drum stems...")
    for track in tracks:
        drum_path = os.path.join(test_dir, track, "drums.wav")
        if not os.path.exists(drum_path):
            continue
            
        # Load and resample to 48000 Hz, mix to mono
        y, sr = librosa.load(drum_path, sr=48000, mono=True)
        
        # Take a 10-second snippet to save memory and time
        if len(y) > sr * 10:
            y = y[sr*5:sr*15]
            
        mag_spec = compute_stft(y)
        
        # Apply extracted mel matrix
        mel_spec = np.dot(mel_matrix, mag_spec)
        all_mel_specs.append(mel_spec)
        
    if not all_mel_specs:
        print("No drum stems loaded!")
        return
        
    # Concatenate along time axis
    V = np.concatenate(all_mel_specs, axis=1)
    
    # Add a small epsilon to avoid zero divisions
    V = np.maximum(V, 1e-12)
    
    K = 3
    tau = 8
    
    W, H = run_nmfd(V, K=K, tau=tau, n_iter=30)
    
    # The required layout is [mel][component][tau]
    # Current W shape is (M, K, tau) which matches [mel][component][tau] in memory!
    # Let's verify shape
    print(f"W shape: {W.shape}") # Should be (128, 3, 8)
    
    out_bin = "w_drums_proto.bin"
    with open(out_bin, "wb") as f:
        # Flatten and write as f32
        f.write(W.flatten().astype(np.float32).tobytes())
    print(f"Saved templates to {out_bin}")
    
    # Plotting
    fig, axes = plt.subplots(1, K, figsize=(15, 5))
    for k in range(K):
        # Component is W[:, k, :], shape (128, 8)
        template = W[:, k, :]
        im = axes[k].imshow(template, origin='lower', aspect='auto', cmap='viridis', interpolation='nearest')
        axes[k].set_title(f"Component {k}")
        axes[k].set_xlabel("Tau (frames)")
        axes[k].set_ylabel("Mel Band")
        fig.colorbar(im, ax=axes[k])
        
    plt.tight_layout()
    plt.savefig("w_drums_proto.png", dpi=150)
    print("Saved plot to w_drums_proto.png")

if __name__ == "__main__":
    main()
