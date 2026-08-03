import os
import glob
import subprocess
import numpy as np
import soundfile as sf
import museval
from scipy.optimize import linear_sum_assignment

def load_wav(path, make_stereo=False):
    data, _ = sf.read(path)
    if data.ndim == 1:
        data = np.expand_dims(data, axis=1)
    if make_stereo and data.shape[1] == 1:
        data = np.concatenate([data, data], axis=1)
    return data

def mse(a, b):
    return np.mean((a - b)**2)

def eval_track(track_dir, k, config_name, window_size):
    mixture_path = os.path.join(track_dir, "mixture.wav")
    extractor_bin = "target/release/oracle_extract"
    subprocess.run([extractor_bin, mixture_path, track_dir], check=True, capture_output=True)
    
    gt_vocals = load_wav(os.path.join(track_dir, "vocals.wav"))
    gt_bass = load_wav(os.path.join(track_dir, "bass.wav"))
    gt_drums = load_wav(os.path.join(track_dir, "drums.wav"))
    gt_other = load_wav(os.path.join(track_dir, "other.wav"))
    
    comps = []
    for i in range(k):
        comps.append(load_wav(os.path.join(track_dir, f"comp{i}.wav"), make_stereo=True))
        
    est_vocals = sum(comps[0:4])
    free_slots = list(range(4, k))
    roles = ['bass', 'drums', 'other']
    gt_refs = [gt_bass, gt_drums, gt_other]
    
    cost_matrix = np.zeros((len(free_slots), 3))
    for i, slot_idx in enumerate(free_slots):
        slot_audio = comps[slot_idx]
        for j, ref_audio in enumerate(gt_refs):
            cost_matrix[i, j] = mse(slot_audio, ref_audio)
            
    row_ind, col_ind = linear_sum_assignment(cost_matrix)
    est_dict = {'vocals': est_vocals, 'bass': np.zeros_like(gt_bass), 'drums': np.zeros_like(gt_drums), 'other': np.zeros_like(gt_other)}
    
    assigned_slots = set()
    for r, c in zip(row_ind, col_ind):
        slot_idx = free_slots[r]
        role = roles[c]
        est_dict[role] = comps[slot_idx]
        assigned_slots.add(slot_idx)
        
    leftover = set(free_slots) - assigned_slots
    for l in leftover:
        est_dict['other'] += comps[l]
        
    target_names = ['vocals', 'bass', 'drums', 'other']
    references = np.stack([gt_vocals, gt_bass, gt_drums, gt_other], axis=0)
    estimates = np.stack([est_dict['vocals'], est_dict['bass'], est_dict['drums'], est_dict['other']], axis=0)
    
    sdr, _, _, _ = museval.evaluate(references, estimates)
    
    for i, name in enumerate(target_names):
        print(f"SWEEP|window={window_size}|track={os.path.basename(track_dir)}|role={name}|sdr={np.nanmedian(sdr[i]):.2f}", flush=True)

def main():
    base_dir = "research/musdb-lab/excerpts"
    tracks = sorted(glob.glob(f"{base_dir}/*"))
    window_size = int(os.environ["WINDOW_SIZE"])
    
    os.environ["NMFD_K"] = "8"
    os.environ["NMFD_ITER"] = "12"
    
    for track_dir in tracks:
        if not os.path.isdir(track_dir):
            continue
        eval_track(track_dir, 8, "CONFIG_A", window_size)

if __name__ == '__main__':
    main()
