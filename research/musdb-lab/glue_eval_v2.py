import numpy as np
import soundfile as sf
import scipy.signal as signal

def load_wav_mono(path):
    data, sr = sf.read(path)
    if data.ndim > 1:
        data = np.mean(data, axis=1)
    return data, sr

def get_nmfd_other(base_path):
    comps = []
    sr = None
    for i in range(4, 8):
        data, sr_c = load_wav_mono(f"{base_path}/comp{i}.wav")
        sr = sr_c
        comps.append(data)
    mat = sum(comps)
    return mat, sr

def filter_biquad_mono(audio, b, a):
    return signal.lfilter(b, a, audio)

def filter_biquad_stereo(audio, b, a):
    out = np.zeros_like(audio)
    out[:, 0] = signal.lfilter(b, a, audio[:, 0])
    out[:, 1] = signal.lfilter(b, a, audio[:, 1])
    return out

def lpf_8k(audio, fs):
    b, a = signal.butter(2, 8000.0 / (fs / 2.0), btype='low')
    return filter_biquad_mono(audio, b, a)

def hpf_order(audio, fs, cutoff=150.0, order=2):
    b, a = signal.butter(order, cutoff / (fs / 2.0), btype='high')
    return filter_biquad_stereo(audio, b, a)

def upmix_decorr(mono, fs):
    # Decorrelation on Side channel (mono-compatible upmix)
    # M = mono, S = Allpass(mono)
    def schroeder_allpass(x, delay_ms, g, fs):
        D = int(fs * delay_ms / 1000.0)
        y = np.zeros_like(x)
        b = np.zeros(D + 1)
        a = np.zeros(D + 1)
        b[0] = g; b[-1] = 1.0
        a[0] = 1.0; a[-1] = g
        return signal.lfilter(b, a, x)
    
    S = schroeder_allpass(mono, 12.0, 0.6, fs)
    S *= 1.5 # Widen to 150%
    M = mono
    return np.column_stack((M + S, M - S))

def upmix_haas(mono, fs):
    # Haas: Delay one channel directly
    L = mono
    R = np.zeros_like(mono)
    delay_samples = int(fs * 15.0 / 1000.0)
    R[delay_samples:] = mono[:-delay_samples]
    
    # Still need 150% width, Haas naturally is wide, but we match the MS width logic:
    M = (L + R) / 2.0
    S = (L - R) / 2.0
    S *= 1.5
    return np.column_stack((M + S, M - S))

def upmix_reverb(mono, fs):
    # Synthetic short reverb on Side channel
    S = np.zeros_like(mono)
    for delay_ms, gain in [(11, 0.5), (17, 0.3), (23, 0.2)]:
        D = int(fs * delay_ms / 1000.0)
        delayed = np.zeros_like(mono)
        delayed[D:] = mono[:-D]
        S += delayed * gain
        
    M = mono
    S *= 1.5
    return np.column_stack((M + S, M - S))

def jelly_sat(audio, drive):
    return np.tanh(audio * drive) / drive

def band_energy_mono(audio, fs, f_low, f_high):
    N = len(audio)
    fft_val = np.fft.rfft(audio)
    freqs = np.fft.rfftfreq(N, 1/fs)
    idx = (freqs >= f_low) & (freqs <= f_high)
    return np.sum(np.abs(fft_val[idx])**2) / N

def measure_comb_notch(mono_unwidened, mono_widened, fs):
    N = len(mono_unwidened)
    f_unw = np.abs(np.fft.rfft(mono_unwidened))
    f_wid = np.abs(np.fft.rfft(mono_widened))
    
    window = np.hanning(100)
    window /= window.sum()
    
    f_unw_smooth = np.convolve(f_unw, window, mode='same')
    f_wid_smooth = np.convolve(f_wid, window, mode='same')
    
    eps = 1e-9
    ratio_db = 20 * np.log10((f_wid_smooth + eps) / (f_unw_smooth + eps))
    freqs = np.fft.rfftfreq(N, 1/fs)
    idx = (freqs >= 20) & (freqs <= 16000)
    
    min_notch = np.min(ratio_db[idx])
    return min_notch

def measure_thd(drive):
    fs = 48000
    t = np.arange(0, 1.0, 1/fs)
    f0 = 1000.0
    sine = np.sin(2 * np.pi * f0 * t)
    sine_st = np.column_stack((sine, sine))
    sat = jelly_sat(sine_st, drive)
    sat_mono = sat[:, 0]
    
    N = len(sat_mono)
    fft_val = np.abs(np.fft.rfft(sat_mono))
    freqs = np.fft.rfftfreq(N, 1/fs)
    
    idx_f0 = np.argmin(np.abs(freqs - f0))
    e_f0 = fft_val[idx_f0]**2
    
    e_harm = 0
    for h in [2, 3, 4, 5]:
        idx_h = np.argmin(np.abs(freqs - h*f0))
        e_harm += fft_val[idx_h]**2
        
    thd = np.sqrt(e_harm) / np.sqrt(e_f0)
    return thd * 100.0

def main():
    base_path = "research/musdb-lab/excerpts/Al_James_-_Schoolboy_Facination"
    mono_mat, fs = get_nmfd_other(base_path)
    
    hf_raw = 10 * np.log10(band_energy_mono(mono_mat, fs, 8000, 20000) + 1e-12)
    step1_lpf = lpf_8k(mono_mat, fs)
    hf_lpf = 10 * np.log10(band_energy_mono(step1_lpf, fs, 8000, 20000) + 1e-12)
    print(f"JURY|noise|stage=raw|hf_energy_db={hf_raw:.2f}")
    print(f"JURY|noise|stage=after_lpf|hf_energy_db={hf_lpf:.2f}")
    
    energy_unwidened = np.sum(step1_lpf**2)
    
    methods = [
        ("decorr", upmix_decorr),
        ("haas", upmix_haas),
        ("reverb_side", upmix_reverb)
    ]
    
    upmixed = {}
    for name, func in methods:
        st = func(step1_lpf, fs) # Use LPF'd version for the chain
        upmixed[name] = st
        
        folded = (st[:, 0] + st[:, 1]) / 2.0
        energy_folded = np.sum(folded**2)
        loss = 10 * np.log10((energy_folded + 1e-12) / (energy_unwidened + 1e-12))
        
        notch = measure_comb_notch(step1_lpf, folded, fs)
        
        S_chan = (st[:, 0] - st[:, 1]) / 2.0
        M_chan = (st[:, 0] + st[:, 1]) / 2.0
        side_energy = 10 * np.log10((np.sum(S_chan**2) + 1e-12) / (np.sum(M_chan**2) + 1e-12))
        
        print(f"JURY2|monoupmix|method={name}|mono_loss_db={loss:.2f}|comb_notch_depth_db={notch:.2f}|side_energy_db={side_energy:.2f}")
        
    base_stereo = upmixed["decorr"]
    
    for order in [2, 4]:
        filtered = hpf_order(base_stereo, fs, cutoff=150.0, order=order)
        mono_f = (filtered[:, 0] + filtered[:, 1]) / 2.0
        mud = 10 * np.log10(band_energy_mono(mono_f, fs, 20, 150) + 1e-12)
        print(f"JURY2|hpf|order={order}|below150_db={mud:.2f}")
        
    for d in [1.0, 1.5, 2.0, 3.0]:
        thd = measure_thd(d)
        print(f"JURY2|jelly|drive={d}|thd_proxy_pct={thd:.2f}")

if __name__ == "__main__":
    main()
