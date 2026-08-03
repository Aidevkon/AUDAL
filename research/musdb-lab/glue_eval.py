import numpy as np
import soundfile as sf
import scipy.signal as signal

def load_wav(path):
    data, sr = sf.read(path)
    return data, sr

def get_material(base_path):
    bass, sr = load_wav(f"{base_path}/bass.wav")
    drums, _ = load_wav(f"{base_path}/drums.wav")
    other, _ = load_wav(f"{base_path}/other.wav")
    mat = bass + drums + other
    return mat, sr

def filter_biquad(audio, b, a):
    out = np.zeros_like(audio)
    out[:, 0] = signal.lfilter(b, a, audio[:, 0])
    out[:, 1] = signal.lfilter(b, a, audio[:, 1])
    return out

def lpf_8k(audio, fs):
    b, a = signal.butter(2, 8000.0 / (fs / 2.0), btype='low')
    return filter_biquad(audio, b, a)

def hpf_150(audio, fs):
    b, a = signal.butter(2, 150.0 / (fs / 2.0), btype='high')
    return filter_biquad(audio, b, a)

def widen_ms(audio, width=1.5):
    # Decorrelation via MS
    L = audio[:, 0]
    R = audio[:, 1]
    M = (L + R) / 2.0
    S = (L - R) / 2.0
    S_new = S * width
    out = np.zeros_like(audio)
    out[:, 0] = M + S_new
    out[:, 1] = M - S_new
    return out

def widen_haas(audio, fs, delay_ms=15.0, width=1.5):
    # Delay right channel
    delay_samples = int(fs * delay_ms / 1000.0)
    out = np.zeros_like(audio)
    
    L = audio[:, 0]
    R = audio[:, 1]
    
    # Simple Haas: keep L, delay R, mix slightly to opposing channels to simulate widen
    # But standard Haas effect just delays one side.
    # To get 150% width, we just delay R and maybe boost S.
    # Let's just delay R by 15ms. 
    out[:, 0] = L
    R_delayed = np.zeros_like(R)
    R_delayed[delay_samples:] = R[:-delay_samples]
    out[:, 1] = R_delayed
    # apply 150% ms width to delayed to match width amount
    out = widen_ms(out, width)
    return out

def jelly_sat(audio, drive=2.0):
    return np.tanh(audio * drive) / drive

def band_energy(audio, fs, f_low, f_high):
    # average energy of stereo
    mono = (audio[:, 0] + audio[:, 1]) / 2.0
    N = len(mono)
    fft_val = np.fft.rfft(mono)
    freqs = np.fft.rfftfreq(N, 1/fs)
    idx = (freqs >= f_low) & (freqs <= f_high)
    return np.sum(np.abs(fft_val[idx])**2) / N

def measure_comb_notch(mono_unwidened, mono_widened, fs):
    N = len(mono_unwidened)
    f_unw = np.abs(np.fft.rfft(mono_unwidened))
    f_wid = np.abs(np.fft.rfft(mono_widened))
    
    # smooth to avoid noise zeroes
    window = np.hanning(100)
    window /= window.sum()
    
    f_unw_smooth = np.convolve(f_unw, window, mode='same')
    f_wid_smooth = np.convolve(f_wid, window, mode='same')
    
    eps = 1e-9
    ratio_db = 20 * np.log10((f_wid_smooth + eps) / (f_unw_smooth + eps))
    
    # We only care about deep notches (negative ratio) in audible range (20Hz to 16kHz)
    freqs = np.fft.rfftfreq(N, 1/fs)
    idx = (freqs >= 20) & (freqs <= 16000)
    
    min_notch = np.min(ratio_db[idx])
    return min_notch

def measure_thd(drive=2.0):
    fs = 48000
    t = np.arange(0, 1.0, 1/fs)
    f0 = 1000.0
    sine = np.sin(2 * np.pi * f0 * t)
    # stereo sine
    sine_st = np.column_stack((sine, sine))
    
    sat = jelly_sat(sine_st, drive)
    sat_mono = sat[:, 0]
    
    N = len(sat_mono)
    fft_val = np.abs(np.fft.rfft(sat_mono))
    freqs = np.fft.rfftfreq(N, 1/fs)
    
    # find fundamental
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
    raw_mat, fs = get_material(base_path)
    
    # JUROR 1: NOISE CLEANUP
    hf_raw = 10 * np.log10(band_energy(raw_mat, fs, 8000, 20000) + 1e-12)
    step1_lpf = lpf_8k(raw_mat, fs)
    hf_lpf = 10 * np.log10(band_energy(step1_lpf, fs, 8000, 20000) + 1e-12)
    
    print(f"JURY|noise|stage=raw|hf_energy_db={hf_raw:.2f}")
    print(f"JURY|noise|stage=after_lpf|hf_energy_db={hf_lpf:.2f}")
    
    # JUROR 2: MONO FOLD-DOWN
    mono_unwidened = (raw_mat[:, 0] + raw_mat[:, 1])
    energy_unwidened = np.sum(mono_unwidened**2)
    
    # (a) Decorrelation (M-S)
    dec_mat = widen_ms(raw_mat, 1.5)
    mono_dec = (dec_mat[:, 0] + dec_mat[:, 1])
    energy_dec = np.sum(mono_dec**2)
    loss_dec = 10 * np.log10((energy_dec + 1e-12) / (energy_unwidened + 1e-12))
    notch_dec = measure_comb_notch(mono_unwidened, mono_dec, fs)
    print(f"JURY|mono|method=decorr|mono_loss_db={loss_dec:.2f}|comb_notch_depth_db={notch_dec:.2f}")
    
    # (b) Haas
    haas_mat = widen_haas(raw_mat, fs, 15.0, 1.5)
    mono_haas = (haas_mat[:, 0] + haas_mat[:, 1])
    energy_haas = np.sum(mono_haas**2)
    loss_haas = 10 * np.log10((energy_haas + 1e-12) / (energy_unwidened + 1e-12))
    notch_haas = measure_comb_notch(mono_unwidened, mono_haas, fs)
    print(f"JURY|mono|method=haas|mono_loss_db={loss_haas:.2f}|comb_notch_depth_db={notch_haas:.2f}")
    
    # JUROR 3: MUD CHECK
    step2_hpf = hpf_150(step1_lpf, fs)
    step3_wid = widen_ms(step2_hpf, 1.5)
    full_chain = jelly_sat(step3_wid, 2.0)
    
    mud_raw = 10 * np.log10(band_energy(raw_mat, fs, 20, 150) + 1e-12)
    mud_full = 10 * np.log10(band_energy(full_chain, fs, 20, 150) + 1e-12)
    print(f"JURY|mud|stage=raw|below150_db={mud_raw:.2f}")
    print(f"JURY|mud|stage=full_chain|below150_db={mud_full:.2f}")
    
    thd_pct = measure_thd(2.0)
    print(f"JURY|jelly|thd_proxy_pct={thd_pct:.2f}")

if __name__ == "__main__":
    main()
