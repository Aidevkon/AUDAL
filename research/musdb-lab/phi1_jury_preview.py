import glob
import os
import sys
import csv
import numpy as np
import soundfile as sf
import torch
import torch.nn as nn
import torchaudio

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

class Phi1Model_V3(nn.Module):
    def __init__(self):
        super().__init__()
        self.conv1 = nn.Conv1d(in_channels=64, out_channels=48, kernel_size=11, dilation=1)
        self.conv2 = nn.Conv1d(in_channels=48, out_channels=48, kernel_size=11, dilation=4)
        self.fc1 = nn.Linear(48, 32)
        self.fc2 = nn.Linear(32, 1)
        
    def forward(self, x):
        x = self.conv1(x)
        x = torch.tanh(x)
        x = self.conv2(x)
        x = torch.tanh(x)
        x = x.squeeze(2)
        x = self.fc1(x)
        x = torch.tanh(x)
        x = self.fc2(x)
        return torch.sigmoid(x).squeeze(1)

def load_v3_model(bin_path):
    model = Phi1Model_V3()
    with open(bin_path, "rb") as f:
        c1w = np.frombuffer(f.read(48*64*11*4), dtype=np.float32).reshape(48, 64, 11)
        c1b = np.frombuffer(f.read(48*4), dtype=np.float32)
        c2w = np.frombuffer(f.read(48*48*11*4), dtype=np.float32).reshape(48, 48, 11)
        c2b = np.frombuffer(f.read(48*4), dtype=np.float32)
        f1w = np.frombuffer(f.read(32*48*4), dtype=np.float32).reshape(32, 48)
        f1b = np.frombuffer(f.read(32*4), dtype=np.float32)
        f2w = np.frombuffer(f.read(1*32*4), dtype=np.float32).reshape(1, 32)
        f2b = np.frombuffer(f.read(1*4), dtype=np.float32)

    model.conv1.weight.data = torch.from_numpy(c1w.copy())
    model.conv1.bias.data = torch.from_numpy(c1b.copy())
    model.conv2.weight.data = torch.from_numpy(c2w.copy())
    model.conv2.bias.data = torch.from_numpy(c2b.copy())
    model.fc1.weight.data = torch.from_numpy(f1w.copy())
    model.fc1.bias.data = torch.from_numpy(f1b.copy())
    model.fc2.weight.data = torch.from_numpy(f2w.copy())
    model.fc2.bias.data = torch.from_numpy(f2b.copy())
    model.eval()
    return model

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
    os.makedirs("/tmp/phi1", exist_ok=True)
    sys.stdout = TeeLogger("/tmp/phi1/jury.log")
    sys.stderr = sys.stdout
    
    model = load_v3_model("/tmp/phi1/phi1_v3.bin")
    mel_spec = torchaudio.transforms.MelSpectrogram(
        sample_rate=16000, n_fft=400, win_length=400, hop_length=160, f_min=0, f_max=8000, n_mels=64
    )
    
    print("Evaluating BEDS...")
    beds = glob.glob("/tmp/w7a/beds/*.wav")
    bed_res = []
    with open("/tmp/phi1/jury_beds.csv", "w") as f:
        w = csv.writer(f)
        w.writerow(["bed", "ratio_05", "ratio_06", "ratio_07"])
        for bed in beds:
            name = os.path.basename(bed)
            res = process_audio(bed, model, mel_spec)
            if not res: continue
            ps = np.array([p for t, p in res])
            r05 = (ps > 0.5).mean()
            r06 = (ps > 0.6).mean()
            r07 = (ps > 0.7).mean()
            w.writerow([name, r05, r06, r07])
            bed_res.append((name, r05, r06, r07))
            
    print("Evaluating W6B POSITIVE...")
    w6b = glob.glob("/tmp/w6b/*/mix.wav")
    pos_res = []
    with open("/tmp/phi1/jury_pos.csv", "w") as f:
        w = csv.writer(f)
        w.writerow(["mix", "snr_tag", "ratio_05", "ratio_06", "ratio_07"])
        for mix in w6b:
            tag = mix.split("/")[-2].split("__")[-1].replace("snr", "")
            res = process_audio(mix, model, mel_spec)
            if not res: continue
            ps = np.array([p for t, p in res])
            r05 = (ps > 0.5).mean()
            r06 = (ps > 0.6).mean()
            r07 = (ps > 0.7).mean()
            name = mix.split("/")[-2]
            w.writerow([name, tag, r05, r06, r07])
            pos_res.append({'name': name, 'tag': tag, 'r05': r05, 'r06': r06, 'r07': r07})

    print("Evaluating FIXTURES (-15 stress)...")
    fixts = [f for f in glob.glob("/tmp/w7a/fixtures/*.wav") if "-15" in f]
    fixt_res = []
    with open("/tmp/phi1/jury_fixtures.csv", "w") as f:
        w = csv.writer(f)
        w.writerow(["fixture", "in_05", "out_05", "in_07", "out_07"])
        for fixt in fixts:
            name = os.path.basename(fixt)
            res = process_audio(fixt, model, mel_spec)
            if not res: continue
            # window [10,20]s
            in_window = [p for t, p in res if 10 <= t <= 20]
            out_window = [p for t, p in res if not (10 <= t <= 20)]
            in_05 = (np.array(in_window) > 0.5).mean() if in_window else 0.0
            out_05 = (np.array(out_window) > 0.5).mean() if out_window else 0.0
            in_07 = (np.array(in_window) > 0.7).mean() if in_window else 0.0
            out_07 = (np.array(out_window) > 0.7).mean() if out_window else 0.0
            w.writerow([name, in_05, out_05, in_07, out_07])
            fixt_res.append({'name': name, 'in05': in_05, 'out05': out_05, 'in07': in_07, 'out07': out_07})

    print("\n--- ΑΝΑΛΥΣΗ ---")
    print("α) BEDS:")
    for thr_idx, thr in [(1, '0.5'), (2, '0.6'), (3, '0.7')]:
        vals = [r[thr_idx] for r in bed_res]
        if not vals: continue
        med = np.median(vals)
        n_good = sum(1 for v in vals if v < 0.05)
        worst = sorted(bed_res, key=lambda x: x[thr_idx], reverse=True)[:3]
        worst_str = ", ".join(f"{w[0]} ({w[thr_idx]:.3f})" for w in worst)
        print(f"  Thr {thr} | median {med:.3f} | <0.05: {n_good}/{len(vals)} | worst: {worst_str}")
    print("  [ΑΝΑΦΟΡΑ] DSP VAD 0.922 median | W7.c gated 0.000")

    print("\nβ) W6B POSITIVE (median ανά threshold):")
    tags = set(r['tag'] for r in pos_res)
    for tag in sorted(tags):
        vals05 = [r['r05'] for r in pos_res if r['tag'] == tag]
        vals06 = [r['r06'] for r in pos_res if r['tag'] == tag]
        vals07 = [r['r07'] for r in pos_res if r['tag'] == tag]
        if vals05:
            print(f"  SNR {tag} | p>0.5: {np.median(vals05):.3f} | p>0.6: {np.median(vals06):.3f} | p>0.7: {np.median(vals07):.3f}")

    print("\nγ) FIXTURES (-15 stress):")
    for r in fixt_res:
        print(f"  {r['name']} | IN [10,20] p>0.5: {r['in05']:>5.3f} (p>0.7: {r['in07']:>5.3f}) | OUT p>0.5: {r['out05']:>5.3f} (p>0.7: {r['out07']:>5.3f})")

    print("\nδ) Η ΚΡΙΣΗ ΩΣ ΝΟΥΜΕΡΑ:")
    print("  Threshold | beds_median | w6b_p3_median")
    print("  ----------|-------------|--------------")
    for thr_idx, thr in [(1, '0.5'), (2, '0.6'), (3, '0.7')]:
        if not bed_res: continue
        bed_med = np.median([r[thr_idx] for r in bed_res])
        w6b_vals = [r['r05'] if thr_idx == 1 else (r['r06'] if thr_idx == 2 else r['r07']) for r in pos_res if r['tag'] == 'p3']
        if w6b_vals:
            w6b_med = np.median(w6b_vals)
            print(f"        {thr} |       {bed_med:.3f} |         {w6b_med:.3f}")

if __name__ == "__main__":
    main()
