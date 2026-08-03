import os
import glob
import numpy as np
import soundfile as sf
import librosa
import torch
from silero_vad import load_silero_vad

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

def extract_features(path, silero_model):
    audio, sr = sf.read(path)
    if audio.ndim > 1:
        audio = np.mean(audio, axis=1)
        
    frame_len = sr
    n_frames = len(audio) // frame_len
    
    # Silero requires 16000 Hz. We will resample the audio to 16kHz to get the VAD probabilities.
    # The excerpt is 48000 Hz, so we just resample.
    audio_16k = librosa.resample(audio, orig_sr=sr, target_sr=16000)
    
    f1_list = []
    f6_list = []
    dropped_frames = 0
    
    for i in range(n_frames):
        start = i * frame_len
        end = start + frame_len
        chunk = audio[start:end]
        
        # 1-second chunk at 16kHz for Silero
        start_16k = i * 16000
        end_16k = start_16k + 16000
        chunk_16k = audio_16k[start_16k:end_16k]
        
        # VAD prediction (Silero)
        # We need to chunk it further as Silero accepts 512 samples.
        # But Silero's predict method can just take the whole 16k tensor and return prob if we use get_speech_timestamps or just loop.
        # Actually `silero_model(tensor, 16000)` might just output the probability of the chunk.
        # Wait, the silero model takes 512 (or 1536) samples. We can just take the max probability over 512-sample chunks in the 1-second frame.
        chunk_tensor = torch.from_numpy(chunk_16k).float()
        
        vad_probs = []
        for j in range(0, len(chunk_tensor), 512):
            sub_chunk = chunk_tensor[j:j+512]
            if len(sub_chunk) == 512:
                prob = silero_model(sub_chunk, 16000).item()
                vad_probs.append(prob)
                
        f1 = np.mean(vad_probs) if vad_probs else 0.0
        
        # F6: Crest Factor with RMS gate (-60 dBFS)
        rms = np.sqrt(np.mean(chunk**2) + 1e-12)
        if 20 * np.log10(rms) < -60:
            dropped_frames += 1
            # don't append to f6_list, but we STILL append to f1_list so the table matches?
            # actually "skip any frame whose RMS is below -60 dBFS, and report how many frames were dropped per file. Recompute f6."
            # Only skipping for f6 is probably meant, but skipping the silent frames entirely makes sense. Let's just skip it for f6.
        else:
            peak = np.max(np.abs(chunk))
            f6 = 20 * np.log10((peak / rms) + 1e-12)
            f6_list.append(f6)
            
        f1_list.append(f1)
        
    f1_mean = np.mean(f1_list) if f1_list else 0.0
    f1_std = np.std(f1_list) if f1_list else 0.0
    
    f6_mean = np.mean(f6_list) if f6_list else 0.0
    f6_std = np.std(f6_list) if f6_list else 0.0
    
    return f1_mean, f1_std, f6_mean, f6_std, dropped_frames

def main():
    files = get_files()
    data = []
    
    # Load Silero
    silero_model = load_silero_vad(onnx=False)
    
    for path, label in files:
        f1_m, f1_s, f6_m, f6_s, dropped = extract_features(path, silero_model)
        data.append({
            'file': os.path.basename(os.path.dirname(path))[:10] + "_" + os.path.basename(path),
            'label': label,
            'f1_mean': f1_m, 'f1_std': f1_s,
            'f6_mean': f6_m, 'f6_std': f6_s,
            'dropped': dropped
        })
        
    print(f"{'FILE':<25} | L | {'f1 (Real VAD)':<18} | {'f6 (Crest Gated)':<18} | Dropped Frames")
    print("-" * 85)
    for d in data:
        row = f"{d['file']:<25} | {d['label']} | "
        row += f"{d['f1_mean']:.2f}(±{d['f1_std']:.2f})    | "
        row += f"{d['f6_mean']:.2f}(±{d['f6_std']:.2f})    | "
        row += f"{d['dropped']}"
        print(row)
        
    print("\n")
    
    classes = ['V', 'D', 'M']
    for name, key in [('f1_real_vad', 'f1_mean'), ('f6_crest_gated', 'f6_mean')]:
        print(f"--- {name} ---")
        for cls in classes:
            vals = [d[key] for d in data if d['label'] == cls]
            print(f"SEP|feature={name}|class={cls}|min={np.min(vals):.3f}|mean={np.mean(vals):.3f}|max={np.max(vals):.3f}")

if __name__ == "__main__":
    main()
