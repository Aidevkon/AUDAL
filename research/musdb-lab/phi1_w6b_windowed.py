import os
import glob
import csv
import sys
import numpy as np
import soundfile as sf
import torch
import torchaudio
from phi1_jury_preview import load_v3_model

class TeeLogger:
    def __init__(self, filename):
        self.terminal = sys.stdout
        self.log = open(filename, "w")
    def write(self, message):
        self.terminal.write(message)
        self.log.write(message)
        self.log.flush()
    def flush(self):
        self.terminal.flush()
        self.log.flush()

def process_audio(path, model, mel_spec_fn):
    audio, sr = sf.read(path)
    if len(audio.shape) > 1:
        audio = audio.mean(axis=1)
    
    audio_t = torch.from_numpy(audio).float()
    if sr != 16000:
        resample = torchaudio.transforms.Resample(orig_freq=sr, new_freq=16000)
        audio_t = resample(audio_t)
        
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
        times = []
        for i in range(50, T):
            window = mel[:, i-50:i+1]
            xs.append(window.numpy())
            times.append(chunk_start_s + i * 0.01)
            
        xs = torch.from_numpy(np.array(xs)).float()
        with torch.no_grad():
            preds = model(xs).numpy()
            
        for t, p in zip(times, preds):
            results.append((t, float(p)))
            
    return results

def main():
    # Load model
    sys.path.append("/home/aidevcon/Documents/creator-os/research/musdb-lab")
    model = load_v3_model("/tmp/phi1/phi1_v3.bin")
    mel_spec = torchaudio.transforms.MelSpectrogram(
        sample_rate=16000, n_fft=400, win_length=400, hop_length=160, f_min=0, f_max=8000, n_mels=64
    )
    
    w6b = glob.glob("/tmp/w6b/*/mix.wav")
    pos_res = []
    
    with open("/tmp/phi1/jury_pos_windowed.csv", "w") as f:
        w = csv.writer(f)
        w.writerow(["mix", "snr_tag", "in_05", "in_07", "out_05", "out_07"])
        for mix in w6b:
            tag = mix.split("/")[-2].split("__")[-1].replace("snr", "")
            name = mix.split("/")[-2]
            res = process_audio(mix, model, mel_spec)
            if not res: continue
            
            in_window = [p for t, p in res if 10 <= t <= 20]
            out_window = [p for t, p in res if not (10 <= t <= 20)]
            
            in_05 = (np.array(in_window) > 0.5).mean() if in_window else 0.0
            out_05 = (np.array(out_window) > 0.5).mean() if out_window else 0.0
            in_07 = (np.array(in_window) > 0.7).mean() if in_window else 0.0
            out_07 = (np.array(out_window) > 0.7).mean() if out_window else 0.0
            
            w.writerow([name, tag, in_05, in_07, out_05, out_07])
            pos_res.append({'name': name, 'tag': tag, 'in05': in_05, 'in07': in_07, 'out05': out_05, 'out07': out_07})

    print("α) ανά snr_tag:")
    tags = set(r['tag'] for r in pos_res)
    for tag in sorted(tags):
        in05_med = np.median([r['in05'] for r in pos_res if r['tag'] == tag])
        in07_med = np.median([r['in07'] for r in pos_res if r['tag'] == tag])
        out05_med = np.median([r['out05'] for r in pos_res if r['tag'] == tag])
        out07_med = np.median([r['out07'] for r in pos_res if r['tag'] == tag])
        print(f"  SNR {tag} | in_05: {in05_med:.3f} | in_07: {in07_med:.3f} | out_05: {out05_med:.3f} | out_07: {out07_med:.3f}")

    print("\nβ) Ο ΤΕΛΙΚΟΣ ΠΙΝΑΚΑΣ ΚΡΙΣΗΣ:")
    print("  Thresh | beds_med | p3_in  | p3_out | fix_in | fix_out")
    print("  -------|----------|--------|--------|--------|--------")
    
    # Beds medians from 3π-2 run
    beds_05 = 0.080
    beds_07 = 0.057
    
    # W6B medians
    p3_in05 = np.median([r['in05'] for r in pos_res if r['tag'] == 'p3'])
    p3_in07 = np.median([r['in07'] for r in pos_res if r['tag'] == 'p3'])
    p3_out05 = np.median([r['out05'] for r in pos_res if r['tag'] == 'p3'])
    p3_out07 = np.median([r['out07'] for r in pos_res if r['tag'] == 'p3'])
    
    # Fixtures values from 3π-2 (duck_splice_real_snr-15 as worst-case example or average)
    # duck_splice_synth_snr-15.wav | IN p>0.5: 0.951 (p>0.7: 0.925) | OUT p>0.5: 0.140 (p>0.7: 0.059)
    # duck_splice_real_snr-15.wav  | IN p>0.5: 0.938 (p>0.7: 0.907) | OUT p>0.5: 0.098 (p>0.7: 0.043)
    # Using averages:
    fix_in05 = (0.951 + 0.938) / 2
    fix_in07 = (0.925 + 0.907) / 2
    fix_out05 = (0.140 + 0.098) / 2
    fix_out07 = (0.059 + 0.043) / 2
    
    print(f"     0.5 |    {beds_05:.3f} |  {p3_in05:.3f} |  {p3_out05:.3f} |  {fix_in05:.3f} |  {fix_out05:.3f}")
    print(f"     0.7 |    {beds_07:.3f} |  {p3_in07:.3f} |  {p3_out07:.3f} |  {fix_in07:.3f} |  {fix_out07:.3f}")

    print("\nγ) Τα 3 χειρότερα w6b in-window ονομαστικά (πού δυσκολεύεται):")
    worst = sorted(pos_res, key=lambda x: x['in05'])[:3]
    for w in worst:
        print(f"  {w['name']} | in_05: {w['in05']:.3f} (in_07: {w['in07']:.3f})")

if __name__ == "__main__":
    main()
