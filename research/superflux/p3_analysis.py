import numpy as np
from scipy.signal import find_peaks

def analyze_fixture(name, env_path, is_speech=False):
    env = np.frombuffer(open(env_path, 'rb').read(), dtype='<f8')
    hop_length = 512
    sr = 48000
    
    p50 = np.percentile(env, 50)
    p90 = np.percentile(env, 90)
    p99 = np.percentile(env, 99)
    
    # Peak picking: local max above p90, distance of at least ~50ms (5 frames)
    peaks, props = find_peaks(env, height=p90, distance=5)
    peak_amps = props['peak_heights']
    
    # Sort peaks by descending amplitude
    sorted_idx = np.argsort(peak_amps)[::-1]
    sorted_peaks = peaks[sorted_idx]
    sorted_amps = peak_amps[sorted_idx]
    
    if is_speech:
        print(f"SFLUX| {name} Stats - p50: {p50:.4f}, p90: {p90:.4f}, p99: {p99:.4f}")
        top_k = 5
    else:
        top_k = 10
        
    for i in range(min(top_k, len(sorted_peaks))):
        p = sorted_peaks[i]
        amp = sorted_amps[i]
        # Librosa onset_strength output has times_like(S) which corresponds to t * hop / sr
        t_sec = p * hop_length / sr
        print(f"SFLUX| {name} Peak {i+1:2d}: t={t_sec:5.3f}s  amp={amp:.4f}")

def main():
    analyze_fixture("bodleasons_mid", "outputs/bodleasons_mid_envelope.bin", is_speech=False)
    analyze_fixture("clip_speech", "outputs/clip_speech_envelope.bin", is_speech=True)

if __name__ == "__main__":
    main()
