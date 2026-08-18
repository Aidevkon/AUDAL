import os
import soundfile as sf
import numpy as np
import mir_eval
from scipy import signal

def align_and_evaluate():
    orig_dir = "/home/aidevcon/Downloads/DATASET/musdb18hq/test/AM Contra - Heart Peripheral"
    orig_mix, sr = sf.read(os.path.join(orig_dir, "mixture.wav"))
    orig_voice, _ = sf.read(os.path.join(orig_dir, "vocals.wav"))
    orig_bass, _ = sf.read(os.path.join(orig_dir, "bass.wav"))
    
    fix_mix, sr2 = sf.read("/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/tests/fixtures/am_contra_30s.wav")
    
    orig_mono = orig_mix.mean(axis=1) if orig_mix.ndim > 1 else orig_mix
    fix_mono = fix_mix.mean(axis=1) if fix_mix.ndim > 1 else fix_mix
    
    fix_clip = fix_mono[:5*sr]
    fix_clip = fix_clip - np.mean(fix_clip)
    fix_clip = fix_clip / np.linalg.norm(fix_clip)
    
    corr = signal.correlate(orig_mono, fix_clip, mode='valid')
    offset = np.argmax(corr)
    
    length = len(fix_mix)
    gt_voice = orig_voice[offset:offset+length].mean(axis=1)
    gt_bass = orig_bass[offset:offset+length].mean(axis=1)
    
    k8_voice, _ = sf.read("/tmp/k8/voice_k8_v2.wav")
    k8_bass, _ = sf.read("/tmp/k8/bass_k8_v2.wav")
    k14_voice, _ = sf.read("/tmp/k14/voice_k14_v2.wav")
    k14_bass, _ = sf.read("/tmp/k14/bass_k14_v2.wav")
    
    k8_voice = k8_voice.mean(axis=1) if k8_voice.ndim > 1 else k8_voice
    k8_bass = k8_bass.mean(axis=1) if k8_bass.ndim > 1 else k8_bass
    k14_voice = k14_voice.mean(axis=1) if k14_voice.ndim > 1 else k14_voice
    k14_bass = k14_bass.mean(axis=1) if k14_bass.ndim > 1 else k14_bass
    
    k8_voice = k8_voice[:length]
    k8_bass = k8_bass[:length]
    k14_voice = k14_voice[:length]
    k14_bass = k14_bass[:length]
    gt_voice = gt_voice[:length]
    gt_bass = gt_bass[:length]
    
    if np.sum(gt_voice**2) < 1e-4: gt_voice += 1e-5 * np.random.randn(len(gt_voice))
    if np.sum(gt_bass**2) < 1e-4: gt_bass += 1e-5 * np.random.randn(len(gt_bass))
    if np.sum(k8_voice**2) < 1e-4: k8_voice += 1e-5 * np.random.randn(len(k8_voice))
    if np.sum(k8_bass**2) < 1e-4: k8_bass += 1e-5 * np.random.randn(len(k8_bass))
    if np.sum(k14_voice**2) < 1e-4: k14_voice += 1e-5 * np.random.randn(len(k14_voice))
    if np.sum(k14_bass**2) < 1e-4: k14_bass += 1e-5 * np.random.randn(len(k14_bass))
    
    ref_arr = np.vstack([gt_voice, gt_bass])
    est_k8 = np.vstack([k8_voice, k8_bass])
    est_k14 = np.vstack([k14_voice, k14_bass])
    
    sdr_k8, sir_k8, sar_k8, perm_k8 = mir_eval.separation.bss_eval_sources(ref_arr, est_k8, compute_permutation=False)
    sdr_k14, sir_k14, sar_k14, perm_k14 = mir_eval.separation.bss_eval_sources(ref_arr, est_k14, compute_permutation=False)
    
    print("\nRESULTS (dB)")
    print(f"Voice K=8  -> SDR: {sdr_k8[0]:.2f} | SIR: {sir_k8[0]:.2f} | SAR: {sar_k8[0]:.2f}")
    print(f"Voice K=14 -> SDR: {sdr_k14[0]:.2f} | SIR: {sir_k14[0]:.2f} | SAR: {sar_k14[0]:.2f}")
    print(f"Bass  K=8  -> SDR: {sdr_k8[1]:.2f} | SIR: {sir_k8[1]:.2f} | SAR: {sar_k8[1]:.2f}")
    print(f"Bass  K=14 -> SDR: {sdr_k14[1]:.2f} | SIR: {sir_k14[1]:.2f} | SAR: {sar_k14[1]:.2f}")

if __name__ == "__main__":
    align_and_evaluate()
