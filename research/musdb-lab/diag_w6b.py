import os
import glob
import numpy as np
import soundfile as sf
import torch
import torchaudio
from phi1_jury_preview import load_v3_model, Phi1Model_V3

def get_ratio_05(audio_t, model, mel_spec_fn):
    chunk_size = 16000 * 5
    num_samples = len(audio_t)
    results = []
    
    for start_idx in range(0, num_samples, chunk_size):
        end_idx = min(start_idx + chunk_size, num_samples)
        chunk = audio_t[start_idx:end_idx]
        
        if len(chunk) < 16000:
            continue
            
        mel = mel_spec_fn(chunk)
        mel = torch.log(mel + 1e-9)
        mel_mean = mel.mean()
        mel_std = mel.std()
        mel = (mel - mel_mean) / (mel_std + 1e-5)
        
        T = mel.shape[1]
        chunk_start_s = start_idx / 16000.0
        
        if T <= 50:
            continue
            
        xs = []
        for i in range(50, T):
            window = mel[:, i-50:i+1]
            xs.append(window.numpy())
            
        xs = torch.from_numpy(np.array(xs)).float()
        with torch.no_grad():
            preds = model(xs).numpy()
            
        for p in preds:
            results.append(float(p))
            
    if not results:
        return 0.0
    ps = np.array(results)
    return (ps > 0.5).mean()

def process_file(path, model, mel_spec_fn):
    audio, sr = sf.read(path)
    if len(audio.shape) > 1:
        audio = audio.mean(axis=1)
    
    audio_t = torch.from_numpy(audio).float()
    if sr != 16000:
        resample = torchaudio.transforms.Resample(orig_freq=sr, new_freq=16000)
        audio_t = resample(audio_t)
    return get_ratio_05(audio_t, model, mel_spec_fn)

def get_ratio_per_chunk(path, model, mel_spec_fn):
    audio, sr = sf.read(path)
    if len(audio.shape) > 1:
        audio = audio.mean(axis=1)
    
    audio_t = torch.from_numpy(audio).float()
    if sr != 16000:
        resample = torchaudio.transforms.Resample(orig_freq=sr, new_freq=16000)
        audio_t = resample(audio_t)
        
    chunk_size = 16000 * 5
    num_samples = len(audio_t)
    chunk_ratios = []
    
    for start_idx in range(0, num_samples, chunk_size):
        end_idx = min(start_idx + chunk_size, num_samples)
        chunk = audio_t[start_idx:end_idx]
        if len(chunk) < 16000:
            continue
        r = get_ratio_05(chunk, model, mel_spec_fn)
        chunk_ratios.append(r)
    return chunk_ratios

def main():
    model = load_v3_model("/tmp/phi1/phi1_v3.bin")
    mel_spec = torchaudio.transforms.MelSpectrogram(
        sample_rate=16000, n_fft=400, win_length=400, hop_length=160, f_min=0, f_max=8000, n_mels=64
    )
    
    print("α) CONTROL 1: speech_segment.flac")
    c1 = process_file("/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/tests/fixtures/duck_splice/speech_segment.flac", model, mel_spec)
    print(f"ratio_05: {c1:.4f}")
    
    p10_mixes = glob.glob("/tmp/w6b/*__snrp10/mix.wav")
    if not p10_mixes:
        print("No p10 mixes found.")
        return
        
    p10_mixes.sort()
    first_mix_dir = os.path.dirname(p10_mixes[0])
    
    print(f"\nβ) CONTROL 2: vocals.wav from {os.path.basename(first_mix_dir)}")
    v_path = os.path.join(first_mix_dir, os.path.basename(first_mix_dir), "vocals.wav")
    c2 = process_file(v_path, model, mel_spec)
    print(f"ratio_05: {c2:.4f}")
    
    print(f"\nγ) CONTROL 3: bed (drums+bass+other) from {os.path.basename(first_mix_dir)}")
    a_d, sr_d = sf.read(os.path.join(first_mix_dir, os.path.basename(first_mix_dir), "drums.wav"))
    a_b, sr_b = sf.read(os.path.join(first_mix_dir, os.path.basename(first_mix_dir), "bass.wav"))
    a_o, sr_o = sf.read(os.path.join(first_mix_dir, os.path.basename(first_mix_dir), "other.wav"))
    sum_audio = a_d + a_b + a_o
    if len(sum_audio.shape) > 1:
        sum_audio = sum_audio.mean(axis=1)
    sum_t = torch.from_numpy(sum_audio).float()
    if sr_d != 16000:
        res = torchaudio.transforms.Resample(orig_freq=sr_d, new_freq=16000)
        sum_t = res(sum_t)
    c3 = get_ratio_05(sum_t, model, mel_spec)
    print(f"ratio_05: {c3:.4f}")
    
    print("\nδ) PER-MIX πίνακας (όλα τα p10):")
    for m in p10_mixes:
        r = process_file(m, model, mel_spec)
        print(f"  {os.path.basename(os.path.dirname(m))}: {r:.4f}")
        
    print(f"\nε) PER-CHUNK profile ΕΝΟΣ p10 mix ({os.path.basename(first_mix_dir)}):")
    cr = get_ratio_per_chunk(p10_mixes[0], model, mel_spec)
    print(f"  chunks ratio_05: {cr}")

if __name__ == "__main__":
    main()
