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
        data = np.expand_dims(data, axis=1) # (nsampl, nchan)
    if make_stereo and data.shape[1] == 1:
        data = np.concatenate([data, data], axis=1)
    return data

def mse(a, b):
    return np.mean((a - b)**2)

def main():
    base_dir = "research/musdb-lab/excerpts"
    tracks = sorted(glob.glob(f"{base_dir}/*"))
    
    extractor_bin = "target/release/oracle_extract"
    
    for track_dir in tracks:
        if not os.path.isdir(track_dir):
            continue
        track_name = os.path.basename(track_dir)
        print(f"\n=== Evaluating Phase A: {track_name} ===")
        
        mixture_path = os.path.join(track_dir, "mixture.wav")
        # 1. Run extractor
        subprocess.run([extractor_bin, mixture_path, track_dir], check=True)
        
        # 2. Load stems
        gt_vocals = load_wav(os.path.join(track_dir, "vocals.wav"))
        gt_bass = load_wav(os.path.join(track_dir, "bass.wav"))
        gt_drums = load_wav(os.path.join(track_dir, "drums.wav"))
        gt_other = load_wav(os.path.join(track_dir, "other.wav"))
        
        comps = []
        for i in range(8):
            comps.append(load_wav(os.path.join(track_dir, f"comp{i}.wav"), make_stereo=True))
            
        # voice = sum(0-3)
        est_vocals = sum(comps[0:4])
        
        # 3. Hungarian match slots 4-7 to [bass, drums, other]
        free_slots = [4, 5, 6, 7]
        roles = ['bass', 'drums', 'other']
        gt_refs = [gt_bass, gt_drums, gt_other]
        
        # Cost matrix: 4 slots x 3 roles
        cost_matrix = np.zeros((4, 3))
        for i, slot_idx in enumerate(free_slots):
            slot_audio = comps[slot_idx]
            for j, ref_audio in enumerate(gt_refs):
                cost_matrix[i, j] = mse(slot_audio, ref_audio)
                
        row_ind, col_ind = linear_sum_assignment(cost_matrix)
        
        est_dict = {'vocals': est_vocals, 'bass': np.zeros_like(gt_bass), 'drums': np.zeros_like(gt_drums), 'other': np.zeros_like(gt_other)}
        
        assignment_log = []
        # Assign matched slots
        assigned_slots = set()
        for r, c in zip(row_ind, col_ind):
            slot_idx = free_slots[r]
            role = roles[c]
            est_dict[role] = comps[slot_idx]
            assigned_slots.add(slot_idx)
            assignment_log.append(f"Slot {slot_idx} -> {role}")
            
        # The 1 leftover slot goes to 'other' (the standard fallback for NMFD unassigned)
        leftover = set(free_slots) - assigned_slots
        for l in leftover:
            est_dict['other'] += comps[l]
            assignment_log.append(f"Slot {l} -> other (leftover)")
            
        print("Assignments:", ", ".join(assignment_log))
        
        # 4. Museval evaluate
        # museval.evaluate(references, estimates) expects shape (nsrc, nsampl, nchan)
        # We must align the order of sources
        target_names = ['vocals', 'bass', 'drums', 'other']
        references = np.stack([gt_vocals, gt_bass, gt_drums, gt_other], axis=0)
        estimates = np.stack([est_dict['vocals'], est_dict['bass'], est_dict['drums'], est_dict['other']], axis=0)
        
        sdr, isr, sir, sar = museval.evaluate(references, estimates)
        
        for i, name in enumerate(target_names):
            sdr_val = np.nanmedian(sdr[i])
            sir_val = np.nanmedian(sir[i])
            sar_val = np.nanmedian(sar[i])
            print(f"Role {name:10}: SDR = {sdr_val:6.2f} dB, SIR = {sir_val:6.2f} dB, SAR = {sar_val:6.2f} dB")

if __name__ == "__main__":
    main()
