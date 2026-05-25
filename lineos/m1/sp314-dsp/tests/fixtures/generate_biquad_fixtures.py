import numpy as np
import scipy.signal
import json
import os
import math

def rbj_peaking(f0, Q, gain_db, Fs):
    f0, Q, gain_db, Fs = map(np.float64, (f0, Q, gain_db, Fs))
    A = np.float64(10.0) ** (gain_db / np.float64(40.0))
    w0 = np.float64(2.0 * np.pi) * f0 / Fs
    cos_w = np.cos(w0, dtype=np.float64)
    sin_w = np.sin(w0, dtype=np.float64)
    alpha = sin_w / (np.float64(2.0) * Q)
    
    b0 = np.float64(1.0) + alpha * A
    b1 = np.float64(-2.0) * cos_w
    b2 = np.float64(1.0) - alpha * A
    a0 = np.float64(1.0) + alpha / A
    a1 = np.float64(-2.0) * cos_w
    a2 = np.float64(1.0) - alpha / A
    
    return [b0/a0, b1/a0, b2/a0, a0/a0, a1/a0, a2/a0]

def rbj_lowshelf(f0, Q, gain_db, Fs):
    f0, Q, gain_db, Fs = map(np.float64, (f0, Q, gain_db, Fs))
    A = np.float64(10.0) ** (gain_db / np.float64(40.0))
    w0 = np.float64(2.0 * np.pi) * f0 / Fs
    cos_w = np.cos(w0, dtype=np.float64)
    sin_w = np.sin(w0, dtype=np.float64)
    alpha = sin_w / (np.float64(2.0) * Q)
    sqA = np.sqrt(A, dtype=np.float64)
    
    b0 = A * ((A + np.float64(1.0)) - (A - np.float64(1.0)) * cos_w + np.float64(2.0) * sqA * alpha)
    b1 = np.float64(2.0) * A * ((A - np.float64(1.0)) - (A + np.float64(1.0)) * cos_w)
    b2 = A * ((A + np.float64(1.0)) - (A - np.float64(1.0)) * cos_w - np.float64(2.0) * sqA * alpha)
    a0 = (A + np.float64(1.0)) + (A - np.float64(1.0)) * cos_w + np.float64(2.0) * sqA * alpha
    a1 = np.float64(-2.0) * ((A - np.float64(1.0)) + (A + np.float64(1.0)) * cos_w)
    a2 = (A + np.float64(1.0)) + (A - np.float64(1.0)) * cos_w - np.float64(2.0) * sqA * alpha
    
    return [b0/a0, b1/a0, b2/a0, a0/a0, a1/a0, a2/a0]

def rbj_highshelf(f0, Q, gain_db, Fs):
    f0, Q, gain_db, Fs = map(np.float64, (f0, Q, gain_db, Fs))
    A = np.float64(10.0) ** (gain_db / np.float64(40.0))
    w0 = np.float64(2.0 * np.pi) * f0 / Fs
    cos_w = np.cos(w0, dtype=np.float64)
    sin_w = np.sin(w0, dtype=np.float64)
    alpha = sin_w / (np.float64(2.0) * Q)
    sqA = np.sqrt(A, dtype=np.float64)
    
    b0 = A * ((A + np.float64(1.0)) + (A - np.float64(1.0)) * cos_w + np.float64(2.0) * sqA * alpha)
    b1 = np.float64(-2.0) * A * ((A - np.float64(1.0)) + (A + np.float64(1.0)) * cos_w)
    b2 = A * ((A + np.float64(1.0)) + (A - np.float64(1.0)) * cos_w - np.float64(2.0) * sqA * alpha)
    a0 = (A + np.float64(1.0)) - (A - np.float64(1.0)) * cos_w + np.float64(2.0) * sqA * alpha
    a1 = np.float64(2.0) * ((A - np.float64(1.0)) - (A + np.float64(1.0)) * cos_w)
    a2 = (A + np.float64(1.0)) - (A - np.float64(1.0)) * cos_w - np.float64(2.0) * sqA * alpha
    
    return [b0/a0, b1/a0, b2/a0, a0/a0, a1/a0, a2/a0]

def rbj_highpass(f0, Q, Fs):
    f0, Q, Fs = map(np.float64, (f0, Q, Fs))
    w0 = np.float64(2.0 * np.pi) * f0 / Fs
    cos_w = np.cos(w0, dtype=np.float64)
    sin_w = np.sin(w0, dtype=np.float64)
    alpha = sin_w / (np.float64(2.0) * Q)
    
    b0 = (np.float64(1.0) + cos_w) / np.float64(2.0)
    b1 = -(np.float64(1.0) + cos_w)
    b2 = (np.float64(1.0) + cos_w) / np.float64(2.0)
    a0 = np.float64(1.0) + alpha
    a1 = np.float64(-2.0) * cos_w
    a2 = np.float64(1.0) - alpha
    
    return [b0/a0, b1/a0, b2/a0, a0/a0, a1/a0, a2/a0]

def rbj_lowpass(f0, Q, Fs):
    f0, Q, Fs = map(np.float64, (f0, Q, Fs))
    w0 = np.float64(2.0 * np.pi) * f0 / Fs
    cos_w = np.cos(w0, dtype=np.float64)
    sin_w = np.sin(w0, dtype=np.float64)
    alpha = sin_w / (np.float64(2.0) * Q)
    
    b0 = (np.float64(1.0) - cos_w) / np.float64(2.0)
    b1 = np.float64(1.0) - cos_w
    b2 = (np.float64(1.0) - cos_w) / np.float64(2.0)
    a0 = np.float64(1.0) + alpha
    a1 = np.float64(-2.0) * cos_w
    a2 = np.float64(1.0) - alpha
    
    return [b0/a0, b1/a0, b2/a0, a0/a0, a1/a0, a2/a0]

def process_filter(f_type, f0, Q, gain_db, Fs):
    if f_type == "bell":
        coeffs = rbj_peaking(f0, Q, gain_db, Fs)
    elif f_type == "lowshelf":
        coeffs = rbj_lowshelf(f0, Q, gain_db, Fs)
    elif f_type == "highshelf":
        coeffs = rbj_highshelf(f0, Q, gain_db, Fs)
    elif f_type == "highpass":
        coeffs = rbj_highpass(f0, Q, Fs)
        gain_db = None
    elif f_type == "lowpass":
        coeffs = rbj_lowpass(f0, Q, Fs)
        gain_db = None
    
    b = coeffs[0:3]
    a = coeffs[3:6]
    
    # Impulse response
    impulse = np.zeros(16, dtype=np.float64)
    impulse[0] = 1.0
    ir = scipy.signal.lfilter(b, a, impulse)
    
    # Freqz
    test_freqs = np.array([20.0, 100.0, 1000.0, 10000.0], dtype=np.float64)
    w = test_freqs * np.float64(2.0 * np.pi) / np.float64(Fs)
    w_out, h = scipy.signal.freqz(b, a, worN=w)
    
    freq_resp = []
    for i, freq in enumerate(test_freqs):
        mag = np.abs(h[i])
        mag_db = 20.0 * np.log10(mag + 1e-15)
        phase = np.angle(h[i])
        freq_resp.append({
            "hz": float(freq),
            "magnitude_db": float(mag_db),
            "phase_rad": float(phase)
        })
    
    result = {
        "type": f_type,
        "f0": float(f0),
        "q": float(Q)
    }
    if gain_db is not None:
        result["gain_db"] = float(gain_db)
        
    result["coefficients"] = {
        "b0": float(coeffs[0]),
        "b1": float(coeffs[1]),
        "b2": float(coeffs[2]),
        "a1": float(coeffs[4]),
        "a2": float(coeffs[5])
    }
    result["impulse_response"] = ir.tolist()
    result["frequency_response"] = freq_resp
    return result

def main():
    Fs = 48000.0
    f0 = 100.0
    Q = 0.707
    gain_db = 6.0
    
    filters = []
    filters.append(process_filter("bell", f0, Q, gain_db, Fs))
    filters.append(process_filter("lowshelf", f0, Q, gain_db, Fs))
    filters.append(process_filter("highshelf", f0, Q, gain_db, Fs))
    filters.append(process_filter("highpass", f0, Q, None, Fs))
    filters.append(process_filter("lowpass", f0, Q, None, Fs))
    
    output = {
        "sample_rate": Fs,
        "filters": filters
    }
    
    os.makedirs("tests/fixtures", exist_ok=True)
    with open("tests/fixtures/biquad_reference.json", "w") as f:
        json.dump(output, f, indent=2)
        
    print(json.dumps(output, indent=2))

if __name__ == "__main__":
    main()
