import csv
import os
import sys
import numpy as np
import soundfile as sf
import scipy.signal
import shutil
import subprocess

def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    
    os.makedirs('/tmp/w5x', exist_ok=True)
    log_fd = os.open('/tmp/w5x/run.log', os.O_WRONLY | os.O_CREAT | os.O_TRUNC)
    os.dup2(log_fd, sys.stdout.fileno())
    os.dup2(log_fd, sys.stderr.fileno())
    
    csv_path = os.path.join(script_dir, 'results/w5c_excerpts.csv')
    tracks = set()
    with open(csv_path, 'r') as f:
        reader = csv.reader(f)
        header = next(reader)
        for row in reader:
            tracks.add(row[0])
            
    tracks = sorted(list(tracks))
    
    results_csv = "/tmp/w5x/results.csv"
    if os.path.exists(results_csv):
        os.remove(results_csv)
    with open(results_csv, "w") as f:
        f.write("track,branch,role,sdr,sir,sar\n")
    
    for track in tracks:
        print(f"Processing track: {track}")
        mix_path = os.path.join(script_dir, f"excerpts/{track}/mixture.wav")
        drums_path = f"/tmp/w5c/{track}/nmfd8/drums.wav"
        
        mix_data, sr = sf.read(mix_path)
        if mix_data.ndim > 1:
            mix_mono = np.mean(mix_data, axis=1)
        else:
            mix_mono = mix_data
            
        drums_data, _ = sf.read(drums_path)
        if drums_data.ndim > 1:
            drums_mono = drums_data[:, 0]
        else:
            drums_mono = drums_data
            
        min_len = min(len(mix_mono), len(drums_mono))
        mix_mono = mix_mono[:min_len]
        drums_mono = drums_mono[:min_len]
        
        sos100 = scipy.signal.butter(4, 100, 'low', fs=sr, output='sos')
        sos150 = scipy.signal.butter(4, 150, 'low', fs=sr, output='sos')
        sos250 = scipy.signal.butter(4, 250, 'low', fs=sr, output='sos')
        
        basses = {
            'xover100': scipy.signal.sosfilt(sos100, mix_mono),
            'xover150': scipy.signal.sosfilt(sos150, mix_mono),
            'xover250': scipy.signal.sosfilt(sos250, mix_mono),
            'xover150d': scipy.signal.sosfilt(sos150, mix_mono - drums_mono)
        }
        
        for variant, bass_mono in basses.items():
            var_dir = f"/tmp/w5x/{track}/{variant}"
            os.makedirs(var_dir, exist_ok=True)
            
            stereo_bass = np.stack([bass_mono, bass_mono], axis=1)
            sf.write(os.path.join(var_dir, "bass.wav"), stereo_bass, sr, subtype='FLOAT')
            
            for stem in ['vocals', 'drums', 'other']:
                src = f"/tmp/w5c/{track}/nmfd8/{stem}.wav"
                dst = os.path.join(var_dir, f"{stem}.wav")
                shutil.copy(src, dst)
                
            gt_dir = os.path.join(script_dir, f"excerpts/{track}")
            semantic_script = os.path.join(script_dir, "semantic_eval.py")
            subprocess.run([sys.executable, semantic_script, gt_dir, var_dir, "/tmp/w5x/results.csv"], check=True)
            
if __name__ == "__main__":
    main()
