import numpy as np

def jelly_sat(audio, drive):
    return np.tanh(audio * drive) / drive

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
    for d in [0.3, 0.5, 0.7]:
        thd = measure_thd(d)
        print(f"JURY2|jelly|drive={d}|thd_proxy_pct={thd:.2f}")

if __name__ == "__main__":
    main()
