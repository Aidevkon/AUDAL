import sys
import os
import numpy as np
import soundfile as sf
import museval

def load_wav(path, make_stereo=True):
    data, _ = sf.read(path)
    if data.ndim == 1:
        data = np.expand_dims(data, axis=1)
    if make_stereo and data.shape[1] == 1:
        data = np.concatenate([data, data], axis=1)
    return data

def main():
    if len(sys.argv) < 3:
        print("Usage: python semantic_eval.py <track_gt_dir> <branch_dir> [csv_out_path]")
        sys.exit(1)
        
    gt_dir = sys.argv[1]
    branch_dir = sys.argv[2]
    csv_path = sys.argv[3] if len(sys.argv) > 3 else None
    
    track_name = os.path.basename(gt_dir.rstrip('/'))
    branch_name = os.path.basename(branch_dir.rstrip('/'))
    
    stems = ['vocals', 'bass', 'drums', 'other']
    
    gts = {}
    ests = {}
    
    for stem in stems:
        gts[stem] = load_wav(os.path.join(gt_dir, f"{stem}.wav"))
        ests[stem] = load_wav(os.path.join(branch_dir, f"{stem}.wav"))
        
    min_len = min([gts[s].shape[0] for s in stems] + [ests[s].shape[0] for s in stems])
    
    for stem in stems:
        gts[stem] = gts[stem][:min_len, :]
        ests[stem] = ests[stem][:min_len, :]
        
    references = np.stack([gts[stem] for stem in stems], axis=0)
    estimates = np.stack([ests[stem] for stem in stems], axis=0)
    
    sdr, isr, sir, sar = museval.evaluate(references, estimates)
    
    print(f"=== Semantic Eval: {branch_name} ===")
    
    if csv_path:
        with open(csv_path, 'a') as f:
            for i, name in enumerate(stems):
                sdr_val = np.nanmedian(sdr[i])
                sir_val = np.nanmedian(sir[i])
                sar_val = np.nanmedian(sar[i])
                print(f"Role {name:10}: SDR = {sdr_val:6.2f} dB, SIR = {sir_val:6.2f} dB, SAR = {sar_val:6.2f} dB")
                f.write(f"{track_name},{branch_name},{name},{sdr_val:.4f},{sir_val:.4f},{sar_val:.4f}\n")
    else:
        for i, name in enumerate(stems):
            sdr_val = np.nanmedian(sdr[i])
            sir_val = np.nanmedian(sir[i])
            sar_val = np.nanmedian(sar[i])
            print(f"Role {name:10}: SDR = {sdr_val:6.2f} dB, SIR = {sir_val:6.2f} dB, SAR = {sar_val:6.2f} dB")

if __name__ == "__main__":
    main()
