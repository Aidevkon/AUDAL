import os
import sys
import csv
import time
import json
import hashlib
import random
import subprocess
import argparse
import numpy as np
import torch
import torch.nn as nn
import torch.optim as optim
from torch.utils.data import Dataset, DataLoader

# Parse args first
parser = argparse.ArgumentParser()
parser.add_argument("--frontend", choices=["logmel", "pcen"], required=True)
args = parser.parse_args()
FRONTEND = args.frontend

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

def set_seed(seed=314159):
    random.seed(seed)
    np.random.seed(seed)
    torch.manual_seed(seed)
    
class Phi1Dataset(Dataset):
    def __init__(self, manifest_path, split):
        super().__init__()
        self.samples = []
        print(f"Loading {split} dataset into memory...")
        with open(manifest_path, 'r') as f:
            reader = csv.DictReader(f)
            for row in reader:
                if row['split'] == split:
                    d = np.load(row['npy_path'], allow_pickle=True).item()
                    labels = d['labels']
                    if FRONTEND == "logmel":
                        mel = d['mel_log']
                        mean = mel.mean()
                        std = mel.std()
                        mel = (mel - mean) / (std + 1e-5)
                    else:
                        mel_pow = d['mel_pow']
                        M = np.zeros_like(mel_pow)
                        M[:, 0] = mel_pow[:, 0]
                        for t in range(1, mel_pow.shape[1]):
                            M[:, t] = (1 - 0.025) * M[:, t-1] + 0.025 * mel_pow[:, t]
                        mel = (mel_pow / (1e-6 + M)**0.98 + 2.0)**0.5 - 2.0**0.5
                    
                    T = mel.shape[1]
                    for i in range(50, T):
                        window = mel[:, i-50:i+1] # (64, 51)
                        label = labels[i]
                        self.samples.append((window, label))
        print(f"{split} loaded: {len(self.samples)} windows.")
        
    def __len__(self):
        return len(self.samples)
        
    def __getitem__(self, idx):
        x, y = self.samples[idx]
        return torch.from_numpy(x).float(), torch.tensor(y).float()

class Phi1Model(nn.Module):
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

def calc_metrics(outputs, labels):
    preds = (outputs > 0.5).float()
    correct = (preds == labels).sum().item()
    total = labels.size(0)
    
    tp = ((preds == 1) & (labels == 1)).sum().item()
    fp = ((preds == 1) & (labels == 0)).sum().item()
    fn = ((preds == 0) & (labels == 1)).sum().item()
    
    return correct, total, tp, fp, fn

def export_model(model, bin_path, json_path, manifest_path):
    c1w = model.conv1.weight.detach().numpy().astype(np.float32)
    c1b = model.conv1.bias.detach().numpy().astype(np.float32)
    c2w = model.conv2.weight.detach().numpy().astype(np.float32)
    c2b = model.conv2.bias.detach().numpy().astype(np.float32)
    f1w = model.fc1.weight.detach().numpy().astype(np.float32)
    f1b = model.fc1.bias.detach().numpy().astype(np.float32)
    f2w = model.fc2.weight.detach().numpy().astype(np.float32)
    f2b = model.fc2.bias.detach().numpy().astype(np.float32)
    
    with open(bin_path, 'wb') as f:
        f.write(c1w.tobytes(order='C'))
        f.write(c1b.tobytes(order='C'))
        f.write(c2w.tobytes(order='C'))
        f.write(c2b.tobytes(order='C'))
        f.write(f1w.tobytes(order='C'))
        f.write(f1b.tobytes(order='C'))
        f.write(f2w.tobytes(order='C'))
        f.write(f2b.tobytes(order='C'))
        
    sha256_hash = hashlib.sha256()
    with open(bin_path, "rb") as f:
        for byte_block in iter(lambda: f.read(4096), b""):
            sha256_hash.update(byte_block)
    bin_hash = sha256_hash.hexdigest()
    
    try:
        git_hash = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    except:
        git_hash = "unknown"
        
    if FRONTEND == "logmel":
        meta_input = "logmel64x51"
        meta_norm = "clip_meanstd"
    else:
        meta_input = "pcen64x51"
        meta_norm = "pcen_s0.025_a0.98_d2.0_r0.5"

    meta = {
        "arch": "conv48k11d1-conv48k11d4-fc32-fc1",
        "input": meta_input,
        "norm": meta_norm,
        "layout": ["conv1.weight", "conv1.bias", "conv2.weight", "conv2.bias", "fc1.weight", "fc1.bias", "fc2.weight", "fc2.bias"],
        "shapes": {
            "conv1.weight": list(c1w.shape),
            "conv1.bias": list(c1b.shape),
            "conv2.weight": list(c2w.shape),
            "conv2.bias": list(c2b.shape),
            "fc1.weight": list(f1w.shape),
            "fc1.bias": list(f1b.shape),
            "fc2.weight": list(f2w.shape),
            "fc2.bias": list(f2b.shape),
        },
        "sha256": bin_hash,
        "dataset": f"phi1_manifest.csv@{git_hash}"
    }
    
    with open(json_path, 'w') as f:
        json.dump(meta, f, indent=2)

def main():
    os.makedirs("/tmp/phi2", exist_ok=True)
    sys.stdout = TeeLogger(f"/tmp/phi2/train_{FRONTEND}.log")
    sys.stderr = sys.stdout
    
    set_seed(314159)
    manifest_path = "/tmp/phi2/manifest.csv"
    
    train_dataset = Phi1Dataset(manifest_path, "train")
    val_dataset = Phi1Dataset(manifest_path, "val")
    
    train_loader = DataLoader(train_dataset, batch_size=256, shuffle=True, num_workers=0)
    val_loader = DataLoader(val_dataset, batch_size=256, shuffle=False, num_workers=0)
    
    model = Phi1Model()
    optimizer = optim.Adam(model.parameters(), lr=1e-3)
    criterion = nn.BCELoss(reduction='none')
    
    best_val_loss = float('inf')
    patience = 3
    no_improve = 0
    best_state = None
    
    print("\nStarting training...")
    for epoch in range(1, 31):
        t0 = time.time()
        
        model.train()
        train_loss = 0.0
        for x, y in train_loader:
            optimizer.zero_grad()
            out = model(x)
            loss_arr = criterion(out, y)
            weight = torch.where(y == 0, 2.0, 1.0)
            loss = (loss_arr * weight).mean()
            loss.backward()
            optimizer.step()
            train_loss += loss.item() * x.size(0)
        train_loss /= len(train_dataset)
        
        model.eval()
        val_loss = 0.0
        val_corr = 0; val_total = 0
        val_tp = 0; val_fp = 0; val_fn = 0
        with torch.no_grad():
            for x, y in val_loader:
                out = model(x)
                loss_arr = criterion(out, y)
                weight = torch.where(y == 0, 2.0, 1.0)
                loss = (loss_arr * weight).mean()
                val_loss += loss.item() * x.size(0)
                
                c, t, tp, fp, fn = calc_metrics(out, y)
                val_corr += c
                val_total += t
                val_tp += tp
                val_fp += fp
                val_fn += fn
                
        val_loss /= len(val_dataset)
        val_acc = val_corr / val_total
        precision = val_tp / (val_tp + val_fp + 1e-9)
        recall = val_tp / (val_tp + val_fn + 1e-9)
        val_f1 = 2 * precision * recall / (precision + recall + 1e-9)
        
        dt = time.time() - t0
        print(f"Epoch {epoch:2d} | Train Loss: {train_loss:.4f} | Val Loss: {val_loss:.4f} | Val Acc: {val_acc:.4f} | Val F1: {val_f1:.4f} | Time: {dt:.1f}s")
        
        if val_loss < best_val_loss:
            best_val_loss = val_loss
            no_improve = 0
            best_state = {k: v.cpu().clone() for k, v in model.state_dict().items()}
        else:
            no_improve += 1
            if no_improve >= patience:
                print(f"Early stopping at epoch {epoch}!")
                break
                
    print("\nTraining completed.")
    model.load_state_dict(best_state)
    
    bin_path = f"/tmp/phi2/phi2_{FRONTEND}.bin"
    json_path = f"/tmp/phi2/phi2_{FRONTEND}.json"
    export_model(model, bin_path, json_path, manifest_path)

    print(f"Exported best model to {bin_path} and {json_path}")

    # Diagnostics
    print("\nRunning Diagnostics on val...")
    model.eval()
    
    stats = {
        "clean_music_fp": [0, 0],
        "clean_speech_tp": [0, 0],
        "mixed_10_lbl1": [0, 0],
        "mixed_3_lbl1": [0, 0],
        "mixed_-3_lbl1": [0, 0],
        "mixed_-9_lbl1": [0, 0],
        "mixed_3_lbl0": [0, 0],
    }
    
    with open(manifest_path, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            if row['split'] == 'val':
                d = np.load(row['npy_path'], allow_pickle=True).item()
                labels = d['labels']
                if FRONTEND == "logmel":
                    mel = d['mel_log']
                    mean = mel.mean()
                    std = mel.std()
                    mel = (mel - mean) / (std + 1e-5)
                else:
                    mel_pow = d['mel_pow']
                    M = np.zeros_like(mel_pow)
                    M[:, 0] = mel_pow[:, 0]
                    for t in range(1, mel_pow.shape[1]):
                        M[:, t] = (1 - 0.025) * M[:, t-1] + 0.025 * mel_pow[:, t]
                    mel = (mel_pow / (1e-6 + M)**0.98 + 2.0)**0.5 - 2.0**0.5
                
                T = mel.shape[1]
                if T <= 50:
                    continue
                    
                windows = []
                valid_labels = []
                for i in range(50, T):
                    windows.append(mel[:, i-50:i+1])
                    valid_labels.append(labels[i])
                    
                windows = np.array(windows)
                valid_labels = np.array(valid_labels)
                
                with torch.no_grad():
                    inputs = torch.from_numpy(windows).float()
                    out = model(inputs).numpy()
                    
                preds = (out > 0.5).astype(int)
                
                kind = row['kind']
                snr = row['snr_db']
                
                if kind == "clean_music":
                    stats["clean_music_fp"][0] += np.sum(preds == 1)
                    stats["clean_music_fp"][1] += len(preds)
                elif kind == "clean_speech":
                    mask = valid_labels == 1
                    stats["clean_speech_tp"][0] += np.sum(preds[mask] == 1)
                    stats["clean_speech_tp"][1] += np.sum(mask)
                elif kind == "mixed":
                    mask1 = valid_labels == 1
                    if snr == "10":
                        stats["mixed_10_lbl1"][0] += np.sum(preds[mask1] == 1)
                        stats["mixed_10_lbl1"][1] += np.sum(mask1)
                    elif snr == "3":
                        stats["mixed_3_lbl1"][0] += np.sum(preds[mask1] == 1)
                        stats["mixed_3_lbl1"][1] += np.sum(mask1)
                        
                        mask0 = valid_labels == 0
                        stats["mixed_3_lbl0"][0] += np.sum(preds[mask0] == 1) # FP
                        stats["mixed_3_lbl0"][1] += np.sum(mask0)
                    elif snr == "-3":
                        stats["mixed_-3_lbl1"][0] += np.sum(preds[mask1] == 1)
                        stats["mixed_-3_lbl1"][1] += np.sum(mask1)
                    elif snr == "-9":
                        stats["mixed_-9_lbl1"][0] += np.sum(preds[mask1] == 1)
                        stats["mixed_-9_lbl1"][1] += np.sum(mask1)

    print(f"metric | ratio")
    for k, v in stats.items():
        ratio = v[0] / max(v[1], 1)
        print(f"{k} | {ratio:.4f}")

if __name__ == "__main__":
    main()
