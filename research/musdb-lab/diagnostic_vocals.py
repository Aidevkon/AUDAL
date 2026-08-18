import os
import glob
import numpy as np
import librosa
import soundfile as sf

MUSDB_DIR = "/home/aidevcon/Downloads/DATASET/musdb18hq"

def compute_stft(y, sr=48000, n_fft=2048, hop_length=512):
    window = 0.5 - 0.5 * np.cos(2.0 * np.pi * np.arange(n_fft) / n_fft)
    D = librosa.stft(y, n_fft=n_fft, hop_length=hop_length, window=window, center=True)
    mag = np.abs(D)
    return mag

def main():
    test_dir = os.path.join(MUSDB_DIR, "test")
    tracks = [d for d in os.listdir(test_dir) if os.path.isdir(os.path.join(test_dir, d))][:4]
    
    for track in tracks:
        vocal_path = os.path.join(test_dir, track, "vocals.wav")
        if not os.path.exists(vocal_path):
            continue
            
        y, sr = librosa.load(vocal_path, sr=48000, mono=True)
        # Scan first 60 seconds
        if len(y) > sr * 60:
            y = y[:sr*60]
            
        mag = compute_stft(y)
        
        freqs = librosa.fft_frequencies(sr=48000, n_fft=2048)
        # Find bin indices for 80-250 Hz
        bin_80 = np.argmax(freqs >= 80)
        bin_250 = np.argmax(freqs >= 250)
        
        # Calculate energy in 80-250 band vs >250 band
        energy_low = np.sum(mag[bin_80:bin_250, :], axis=0)
        energy_high = np.sum(mag[bin_250:, :], axis=0)
        total_energy = np.sum(mag, axis=0)
        
        mean_energy = np.mean(total_energy)
        threshold = mean_energy * 0.05
        
        active_mask = total_energy > threshold
        strong_voice_mask = total_energy > (mean_energy * 0.5)
        weak_voice_mask = active_mask & (~strong_voice_mask)
        
        low_energy_when_strong = np.mean(energy_low[strong_voice_mask]) if np.sum(strong_voice_mask) > 0 else 0
        low_energy_when_weak = np.mean(energy_low[weak_voice_mask]) if np.sum(weak_voice_mask) > 0 else 0
        
        high_energy_when_strong = np.mean(energy_high[strong_voice_mask]) if np.sum(strong_voice_mask) > 0 else 0
        high_energy_when_weak = np.mean(energy_high[weak_voice_mask]) if np.sum(weak_voice_mask) > 0 else 0
        
        print(f"Track: {track}")
        print(f"  Strong voice frames: {np.sum(strong_voice_mask)}")
        print(f"    Avg Low-band (80-250) energy: {low_energy_when_strong:.4f}")
        print(f"    Avg High-band (>250) energy:  {high_energy_when_strong:.4f}")
        print(f"    Low/High ratio: {low_energy_when_strong/(high_energy_when_strong+1e-12):.4f}")
        print(f"  Weak voice frames (breaths/bleed): {np.sum(weak_voice_mask)}")
        print(f"    Avg Low-band (80-250) energy: {low_energy_when_weak:.4f}")
        print(f"    Avg High-band (>250) energy:  {high_energy_when_weak:.4f}")
        print(f"    Low/High ratio: {low_energy_when_weak/(high_energy_when_weak+1e-12):.4f}")
        print("-" * 40)

if __name__ == "__main__":
    main()
