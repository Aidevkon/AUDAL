import os
import glob
import numpy as np
import soundfile as sf
import librosa

def get_files():
    base = "research/musdb-lab/excerpts"
    tracks = ["Al_James_-_Schoolboy_Facination", "Forkupines_-_Semantics", "Punkdisco_-_Oral_Hygiene"]
    stems = ["vocals", "drums", "bass", "other"]
    
    files = []
    for track in tracks:
        for stem in stems:
            path = os.path.join(base, track, stem + ".wav")
            label = "V" if stem == "vocals" else ("D" if stem == "drums" else "M")
            files.append((path, label))
    return files

def extract_features(path):
    audio, sr = sf.read(path)
    if audio.ndim > 1:
        audio = np.mean(audio, axis=1)
        
    frame_len = sr
    n_frames = len(audio) // frame_len
    features = []
    
    S = np.abs(librosa.stft(audio, n_fft=2048, hop_length=512))
    H, P = librosa.decompose.hpss(S, margin=1.0)
    mask_p = (P + 1e-8) / (H + P + 1e-8)
    
    freqs = librosa.fft_frequencies(sr=sr, n_fft=2048)
    f1_idx = (freqs >= 300) & (freqs <= 3400)
    f4_idx = (freqs < 150)
    f5_idx = (freqs > 8000)
    
    centroid = librosa.feature.spectral_centroid(S=S, sr=sr)[0]
    flatness = librosa.feature.spectral_flatness(S=S)[0]
    flux = librosa.onset.onset_strength(S=S, sr=sr)
    
    for i in range(n_frames):
        start = i * frame_len
        end = start + frame_len
        chunk = audio[start:end]
        
        t_start = librosa.samples_to_frames(start, hop_length=512)
        t_end = librosa.samples_to_frames(end, hop_length=512)
        
        S_chunk = S[:, t_start:t_end]
        E_chunk = np.sum(S_chunk**2, axis=0) + 1e-12
        
        f1 = np.mean(np.sum(S_chunk[f1_idx, :]**2, axis=0) / E_chunk)
        f2 = np.mean(mask_p[:, t_start:t_end])
        f3 = np.mean(centroid[t_start:t_end])
        f4 = np.mean(np.sum(S_chunk[f4_idx, :]**2, axis=0) / E_chunk)
        f5 = np.mean(np.sum(S_chunk[f5_idx, :]**2, axis=0) / E_chunk)
        
        rms = np.sqrt(np.mean(chunk**2) + 1e-12)
        peak = np.max(np.abs(chunk))
        f6 = 20 * np.log10((peak / rms) + 1e-12)
        
        f7 = np.mean(flatness[t_start:t_end])
        f8 = np.mean(flux[t_start:t_end])
        
        features.append([f1, f2, f3, f4, f5, f6, f7, f8])
        
    features = np.array(features)
    if len(features) == 0:
        return np.zeros(8), np.zeros(8)
    return np.mean(features, axis=0), np.std(features, axis=0)

def main():
    files = get_files()
    data = []
    
    for path, label in files:
        f_mean, f_std = extract_features(path)
        data.append({
            'file': os.path.basename(os.path.dirname(path))[:10] + "_" + os.path.basename(path),
            'label': label,
            'mean': f_mean,
            'std': f_std
        })
        
    # Print Table
    print(f"{'FILE':<25} | L | {'f1 (VAD_px)':<14} | {'f2 (Perc)':<14} | {'f3 (Cent)':<14} | {'f4 (Low)':<14} | {'f5 (High)':<14} | {'f6 (Crest)':<14} | {'f7 (Flat)':<14} | {'f8 (Flux)':<14}")
    print("-" * 155)
    for d in data:
        m = d['mean']
        s = d['std']
        row = f"{d['file']:<25} | {d['label']} | "
        for i in range(8):
            row += f"{m[i]:.2f}(±{s[i]:.2f}) | "
        print(row)
        
    print("\n")
    
    # Print separation
    classes = ['V', 'D', 'M']
    feature_names = ['f1_vad_px', 'f2_perc_ratio', 'f3_centroid', 'f4_low_ratio', 'f5_high_ratio', 'f6_crest', 'f7_flatness', 'f8_flux']
    
    for i, name in enumerate(feature_names):
        print(f"--- {name} ---")
        for cls in classes:
            vals = [d['mean'][i] for d in data if d['label'] == cls]
            print(f"SEP|feature={name}|class={cls}|min={np.min(vals):.3f}|mean={np.mean(vals):.3f}|max={np.max(vals):.3f}")

if __name__ == "__main__":
    main()
