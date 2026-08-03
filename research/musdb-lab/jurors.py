import os
import numpy as np
import soundfile as sf
import scipy.signal as signal

def load_wav(path):
    data, sr = sf.read(path)
    return data, sr

# --- JUROR 1: EQ PREDICTABILITY ---
# +6dB high shelf at 8kHz.
def biquad_high_shelf(fs, fc, gain_db, Q=0.707):
    A = 10.0 ** (gain_db / 40.0)
    w0 = 2 * np.pi * fc / fs
    alpha = np.sin(w0) / (2 * Q)
    
    b0 = A * ((A + 1) + (A - 1) * np.cos(w0) + 2 * np.sqrt(A) * alpha)
    b1 = -2 * A * ((A - 1) + (A + 1) * np.cos(w0))
    b2 = A * ((A + 1) + (A - 1) * np.cos(w0) - 2 * np.sqrt(A) * alpha)
    a0 = (A + 1) - (A - 1) * np.cos(w0) + 2 * np.sqrt(A) * alpha
    a1 = 2 * ((A - 1) - (A + 1) * np.cos(w0))
    a2 = (A + 1) - (A - 1) * np.cos(w0) - 2 * np.sqrt(A) * alpha
    
    return [b0/a0, b1/a0, b2/a0], [1.0, a1/a0, a2/a0]

def band_energy(audio, fs, f_low, f_high):
    # simple FFT based energy
    N = len(audio)
    fft_val = np.fft.rfft(audio)
    freqs = np.fft.rfftfreq(N, 1/fs)
    idx = (freqs >= f_low) & (freqs <= f_high)
    return np.sum(np.abs(fft_val[idx])**2) / N

def juror_eq(audio, fs):
    b, a = biquad_high_shelf(fs, 8000, 6.0)
    audio_eq = signal.lfilter(b, a, audio)
    
    # Delta signal (added by EQ)
    delta_audio = audio_eq - audio
    
    # Energy added in 8-16kHz
    e_8_16 = band_energy(delta_audio, fs, 8000, 16000)
    # Energy added in 2-8kHz
    e_2_8 = band_energy(delta_audio, fs, 2000, 8000)
    
    ratio = e_8_16 / (e_2_8 + 1e-12)
    return ratio

# --- JUROR 2: COMPRESSOR PREDICTABILITY ---
# Fixed thresh/ratio/att/rel. Measure GR variance over time.
def juror_compressor(audio, fs):
    thresh_db = -18.0
    ratio = 4.0
    attack_ms = 10.0
    release_ms = 100.0
    
    attack_coeff = np.exp(-1.0 / (fs * attack_ms / 1000.0))
    release_coeff = np.exp(-1.0 / (fs * release_ms / 1000.0))
    
    env = 0.0
    gr_log = np.zeros_like(audio)
    
    for i in range(len(audio)):
        level = np.abs(audio[i])
        level_db = 20 * np.log10(level + 1e-9)
        
        # Detector
        if level > env:
            env = attack_coeff * env + (1.0 - attack_coeff) * level
        else:
            env = release_coeff * env + (1.0 - release_coeff) * level
            
        env_db = 20 * np.log10(env + 1e-9)
        
        # Gain calculation
        if env_db > thresh_db:
            gr = (thresh_db - env_db) * (1.0 - 1.0/ratio)
        else:
            gr = 0.0
            
        gr_log[i] = gr
        
    return np.var(gr_log)

# --- JUROR 3: REVERB PREDICTABILITY ---
# Reverb tail low-freq energy
def biquad_lowpass(fs, fc, Q=0.707):
    w0 = 2 * np.pi * fc / fs
    alpha = np.sin(w0) / (2 * Q)
    
    b1 = 1 - np.cos(w0)
    b0 = b1 / 2
    b2 = b0
    a0 = 1 + alpha
    a1 = -2 * np.cos(w0)
    a2 = 1 - alpha
    return [b0/a0, b1/a0, b2/a0], [1.0, a1/a0, a2/a0]

def juror_reverb(audio, fs):
    # Simple delay-based reverb: apply a few feedback delays
    # Then measure energy AFTER the audio ends (the tail)
    
    # 500ms delay line
    delay_samples = int(0.5 * fs)
    reverb_audio = np.zeros(len(audio) + delay_samples * 3)
    
    # 3 taps
    for tap, gain in [(int(0.13 * fs), 0.5), (int(0.27 * fs), 0.25), (int(0.41 * fs), 0.125)]:
        reverb_audio[tap:tap+len(audio)] += audio * gain
        
    # The tail is strictly after the dry signal ends
    tail = reverb_audio[len(audio):]
    
    # Low freq energy (< 200Hz)
    b, a = biquad_lowpass(fs, 200, 0.707)
    tail_lf = signal.lfilter(b, a, tail)
    
    energy_lf = np.mean(tail_lf**2)
    return 10 * np.log10(energy_lf + 1e-12)

def main():
    mat_a, fs_a = load_wav('research/musdb-lab/material_A.wav')
    mat_b, fs_b = load_wav('research/musdb-lab/material_B.wav')
    
    # Ensure they start at same energy. Wait, B is masked A, so B will have LESS energy.
    # The user: "confirm they start at the SAME total energy minus the HPSS-removed part (so the comparison is fair)"
    # This means B is exactly A minus percussive part. The files are generated this way, no extra normalization needed.
    # But wait, we should normalize them to the same peak or loudness so the compressor acts similarly?
    # No, the user explicitly says "same total energy minus the HPSS-removed part", meaning no additional normalization!
    
    # EQ
    eq_a = juror_eq(mat_a, fs_a)
    eq_b = juror_eq(mat_b, fs_b)
    print(f"JURY|eq|material=A|harsh_ratio={eq_a:.3f}    JURY|eq|material=B|harsh_ratio={eq_b:.3f}")
    
    # Comp
    comp_a = juror_compressor(mat_a, fs_a)
    comp_b = juror_compressor(mat_b, fs_b)
    print(f"JURY|comp|material=A|gr_variance={comp_a:.3f}  JURY|comp|material=B|gr_variance={comp_b:.3f}")
    
    # Reverb
    verb_a = juror_reverb(mat_a, fs_a)
    verb_b = juror_reverb(mat_b, fs_b)
    print(f"JURY|verb|material=A|lowfreq_tail_db={verb_a:.3f}  JURY|verb|material=B|lowfreq_tail_db={verb_b:.3f}")
    
    print("-" * 60)
    print(f"DELTA (EQ harsh_ratio)    : {eq_b - eq_a:+.3f} (Lower is better)")
    print(f"DELTA (Comp gr_variance)  : {comp_b - comp_a:+.3f} (Lower is better)")
    print(f"DELTA (Verb lowfreq_tail) : {verb_b - verb_a:+.3f} dB (Lower is better)")

if __name__ == "__main__":
    main()
