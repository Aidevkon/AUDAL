import os
import soundfile as sf
import numpy as np

def calculate_rms_db(signal, frame_len):
    """Calculates RMS in dB for non-overlapping windows."""
    n_frames = len(signal) // frame_len
    rms_db = []
    for i in range(n_frames):
        frame = signal[i * frame_len : (i + 1) * frame_len]
        # Calculate RMS
        rms = np.sqrt(np.mean(frame**2))
        # Convert to dB, handling near-zero values
        if rms < 1e-10:
            rms_db.append(-100.0)
        else:
            rms_db.append(20 * np.log10(rms))
    return np.array(rms_db)

def main():
    import scipy.signal as signal
    
    gt_dir = "/home/aidevcon/Downloads/DATASET/musdb18hq/test/AM Contra - Heart Peripheral"
    gt_mix_path = os.path.join(gt_dir, "mixture.wav")
    gt_path = os.path.join(gt_dir, "vocals.wav")
    k8_path = os.path.expanduser("~/nmfd-phase2-backup/voice_k8_v2.wav")
    k14_path = os.path.expanduser("~/nmfd-phase2-backup/voice_k14_v2.wav")
    fix_path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/tests/fixtures/am_contra_30s.wav"
    
    # Load and align
    orig_mix, sr = sf.read(gt_mix_path)
    gt_audio, _ = sf.read(gt_path)
    fix_mix, sr_k = sf.read(fix_path)
    
    orig_mono = orig_mix.mean(axis=1) if orig_mix.ndim > 1 else orig_mix
    fix_mono = fix_mix.mean(axis=1) if fix_mix.ndim > 1 else fix_mix
    
    # Wait, gt is 44.1kHz, fix is 48kHz! 
    # Actually MUSDB is 44.1kHz. Let's see if fix_mix is 44.1kHz or 48kHz.
    # We correlate them directly? eval_bss_mir.py just correlates orig_mono with fix_clip directly?!
    # No, am_contra_30s.wav might be 44.1kHz! Let's check sr and sr2.
    # In eval_bss_mir.py, it doesn't resample. So am_contra_30s.wav is probably also 44.1kHz.
    
    fix_clip = fix_mono[:5*sr_k]
    fix_clip = fix_clip - np.mean(fix_clip)
    fix_clip = fix_clip / np.linalg.norm(fix_clip)
    
    corr = signal.correlate(orig_mono, fix_clip, mode='valid')
    offset = np.argmax(corr)
    
    length = len(fix_mix)
    gt_mono = gt_audio[offset:offset+length].mean(axis=1)
    
    # Load k8 and k14
    k8_audio, _ = sf.read(k8_path, dtype='float32')
    k14_audio, _ = sf.read(k14_path, dtype='float32')
    
    k8_mono = k8_audio.mean(axis=1) if k8_audio.ndim > 1 else k8_audio
    k14_mono = k14_audio.mean(axis=1) if k14_audio.ndim > 1 else k14_audio
    
    # Truncate to length if needed
    k8_mono = k8_mono[:length]
    k14_mono = k14_mono[:length]
    gt_mono = gt_mono[:length]
    
    # We want 30 1-second windows. Frame len = sample rate.
    gt_rms = calculate_rms_db(gt_mono, sr_k)[:30]
    k8_rms = calculate_rms_db(k8_mono, sr_k)[:30]
    k14_rms = calculate_rms_db(k14_mono, sr_k)[:30]
    
    print("Sec |   GT (dB) |   K8 (dB) |  K14 (dB) | State")
    print("----|-----------|-----------|-----------|------")
    
    # Threshold for silence
    # Let's inspect the actual RMS values. Usually vocals are > -40, silence is < -50.
    silence_threshold = -45.0
    
    silent_secs = []
    vocal_secs = []
    
    for i in range(30):
        gt = gt_rms[i]
        k8 = k8_rms[i]
        k14 = k14_rms[i]
        
        state = "VOCAL" if gt > silence_threshold else "SILENT"
        if state == "VOCAL":
            vocal_secs.append(i)
        else:
            silent_secs.append(i)
            
        print(f"{i:2d}  | {gt:9.2f} | {k8:9.2f} | {k14:9.2f} | {state}")
        
    print("\n--- Summary ---")
    
    def mean_db(indices, rms_array):
        if not indices:
            return -100.0
        powers = [10**(rms_array[i] / 10.0) for i in indices]
        mean_power = np.mean(powers)
        if mean_power < 1e-20: return -100.0
        return 10 * np.log10(mean_power)
    
    vocal_gt = mean_db(vocal_secs, gt_rms)
    vocal_k8 = mean_db(vocal_secs, k8_rms)
    vocal_k14 = mean_db(vocal_secs, k14_rms)
    
    silent_gt = mean_db(silent_secs, gt_rms)
    silent_k8 = mean_db(silent_secs, k8_rms)
    silent_k14 = mean_db(silent_secs, k14_rms)
    
    print(f"VOCAL segments ({len(vocal_secs)}s): GT={vocal_gt:.2f}dB, K8={vocal_k8:.2f}dB, K14={vocal_k14:.2f}dB")
    print(f"SILENT segments ({len(silent_secs)}s): GT={silent_gt:.2f}dB, K8={silent_k8:.2f}dB, K14={silent_k14:.2f}dB")
    print(f"\nDifference in SILENT (leakage): K14 vs K8 = {silent_k14 - silent_k8:.2f} dB")

if __name__ == "__main__":
    main()
