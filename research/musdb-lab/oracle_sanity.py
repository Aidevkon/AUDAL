import os
import glob
import numpy as np
import soundfile as sf
import museval

def load_wav(path, make_stereo=False):
    data, _ = sf.read(path)
    if data.ndim == 1:
        data = np.expand_dims(data, axis=1) # (nsampl, nchan)
    if make_stereo and data.shape[1] == 1:
        data = np.concatenate([data, data], axis=1)
    return data

def evaluate_baseline(track_dir):
    print(f"\n=== Sanity Check: {os.path.basename(track_dir)} ===")
    
    gt_vocals = load_wav(os.path.join(track_dir, "vocals.wav"))
    gt_bass = load_wav(os.path.join(track_dir, "bass.wav"))
    gt_drums = load_wav(os.path.join(track_dir, "drums.wav"))
    gt_other = load_wav(os.path.join(track_dir, "other.wav"))
    
    mixture = load_wav(os.path.join(track_dir, "mixture.wav"))
    
    target_names = ['vocals', 'bass', 'drums', 'other']
    references = np.stack([gt_vocals, gt_bass, gt_drums, gt_other], axis=0)
    
    # Anchor 1: Self vs Self
    estimates_self = np.stack([gt_vocals, gt_bass, gt_drums, gt_other], axis=0)
    print("Evaluating Self vs Self...")
    sdr, isr, sir, sar = museval.evaluate(references, estimates_self)
    for i, name in enumerate(target_names):
        print(f"[Self] Role {name:10}: SDR = {np.nanmedian(sdr[i]):6.2f} dB")
        
    # Anchor 2: Mixture vs References (Baseline)
    estimates_mix = np.stack([mixture, mixture, mixture, mixture], axis=0)
    print("Evaluating Mixture vs Self...")
    sdr_m, isr_m, sir_m, sar_m = museval.evaluate(references, estimates_mix)
    for i, name in enumerate(target_names):
        print(f"[Mixture] Role {name:10}: SDR = {np.nanmedian(sdr_m[i]):6.2f} dB")

def main():
    base_dir = "research/musdb-lab/excerpts"
    tracks = sorted(glob.glob(f"{base_dir}/*"))
    for track_dir in tracks:
        if os.path.isdir(track_dir):
            evaluate_baseline(track_dir)
            break # Just one track is enough for sanity check

if __name__ == "__main__":
    main()
