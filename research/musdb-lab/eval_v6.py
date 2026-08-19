import os
import re
import struct
import numpy as np
import librosa

def parse_mel_matrix():
    rust_path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/src/analysis/mel_128.rs"
    mel_bands = 128
    n_bins = 1025
    matrix = np.zeros((mel_bands, n_bins), dtype=np.float32)
    with open(rust_path, 'r') as f: content = f.read()
    start_idx = content.find("pub const MEL_128_MATRIX")
    end_idx = content.find("pub const MEL_128_INVERSE", start_idx)
    matrix_str = content[start_idx:end_idx]
    hex_vals = re.findall(r'f32::from_bits\((0x[0-9a-fA-F]+)\)', matrix_str)
    for i, hex_str in enumerate(hex_vals):
        band = i // n_bins
        bin_idx = i % n_bins
        int_val = int(hex_str, 16)
        float_val = struct.unpack('>f', struct.pack('>I', int_val))[0]
        matrix[band, bin_idx] = float_val
    return matrix

def load_tensors_14(w_sung_path):
    w_speech_path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/assets/w_speech_v1.bin"
    w_music_path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/assets/w_music_v1.bin"
    
    with open(w_speech_path, "rb") as f:
        w_speech = np.frombuffer(f.read(), dtype=np.float32).reshape(128, 4, 8)
    with open(w_music_path, "rb") as f:
        w_music = np.frombuffer(f.read(), dtype=np.float32).reshape(128, 6, 8)
    with open(w_sung_path, "rb") as f:
        w_sung = np.frombuffer(f.read(), dtype=np.float32).reshape(128, 3, 8)
        
    w_drums = w_music[:, 0:3, :]
    
    W14 = np.zeros((128, 14, 8), dtype=np.float32)
    W14[:, 0:4, :] = w_speech
    W14[:, 4:7, :] = w_drums
    W14[:, 7:10, :] = w_sung
    # Seed 10-13 free slots with small random values to emulate free slots
    np.random.seed(42)
    W14[:, 10:14, :] = np.random.rand(128, 4, 8).astype(np.float32) * 0.1
    for k in range(10, 14):
        W14[:, k, :] /= np.sum(W14[:, k, :])
    return W14

def compute_stft_mel(y, mel_matrix):
    window = 0.5 - 0.5 * np.cos(2.0 * np.pi * np.arange(2048) / 2048)
    D = librosa.stft(y, n_fft=2048, hop_length=512, window=window, center=True)
    mag = np.abs(D)
    return np.dot(mel_matrix, mag)

def run_nmfd_h_only(V, W, n_iter=12):
    M, K, tau = W.shape
    N = V.shape[1]
    H = np.full((K, N), 0.1, dtype=np.float32)
    
    for _ in range(n_iter):
        V_hat = np.zeros_like(V)
        for t in range(tau):
            H_shifted = np.pad(H, ((0,0), (t,0)), mode='constant')[:, :N]
            V_hat += np.dot(W[:, :, t], H_shifted)
        V_hat = np.maximum(V_hat, 1e-12)
        ratio = V / V_hat
        
        num = np.zeros_like(H)
        den = np.zeros_like(H)
        for t in range(tau):
            if t == 0:
                ratio_shifted = ratio
                ones_shifted = np.ones_like(ratio)
            else:
                ratio_shifted = np.pad(ratio[:, t:], ((0,0), (0,t)), mode='constant')
                ones_shifted = np.pad(np.ones_like(ratio[:, t:]), ((0,0), (0,t)), mode='constant')
            num += np.dot(W[:, :, t].T, ratio_shifted)
            den += np.dot(W[:, :, t].T, ones_shifted)
        H = H * num / np.maximum(den, 1e-12)
    return H

def generate_synthetic_strings(duration_sec=30.0, sr=48000):
    t = np.linspace(0, duration_sec, int(sr * duration_sec), endpoint=False)
    vibrato = 5.5 * np.sin(2.0 * np.pi * 5.5 * t)
    f0_list = [110.0, 164.81, 220.0, 329.63]
    sig = np.zeros_like(t)
    
    def formant_weight(freq):
        w = 0.0
        for center in [500.0, 1500.0, 2500.0]:
            w += np.exp(-((freq - center) ** 2) / (2.0 * (150.0 ** 2)))
        return np.maximum(w, 0.05)
        
    for f0 in f0_list:
        for h in range(1, 16):
            freq = h * f0
            phase = 2.0 * np.pi * (freq * t + h * np.cumsum(vibrato) / sr)
            weight = (1.0 / h) * formant_weight(freq)
            sig += weight * np.sin(phase)
            
    max_val = np.max(np.abs(sig))
    if max_val > 0: sig /= max_val
    return sig.astype(np.float32)

def evaluate_w_sung(w_sung_path, name="w_sung_v6"):
    print(f"\n==================================================")
    print(f"EVALUATING SUNG TEMPLATE: {name} ({w_sung_path})")
    print(f"==================================================")
    
    mel_matrix = parse_mel_matrix()
    W14 = load_tensors_14(w_sung_path)
    
    # --------------------------------------------------
    # 1. SYNTHETIC TRACK (/tmp/blue/comp0.wav)
    # --------------------------------------------------
    comp0_path = "/tmp/blue/comp0.wav"
    if os.path.exists(comp0_path):
        y, sr = librosa.load(comp0_path, sr=48000, mono=True)
        if len(y) > sr * 30: y = y[:sr*30]
        V_comp0 = compute_stft_mel(y, mel_matrix)
        H_comp0 = run_nmfd_h_only(V_comp0, W14, n_iter=12)
        mean_h = np.mean(H_comp0, axis=1)
        
        max_drums = np.max(mean_h[4:7])
        c0_routed = mean_h[7]
        c1_soak = mean_h[8]
        c2_retried = mean_h[9]
        max_free = np.max(mean_h[10:14])
        
        r1_drums = max_drums / max(max_free, 1e-12)
        r2_c0 = c0_routed / max(max_free, 1e-12)
        r_soak = c1_soak / max(max_free, 1e-12)
        r_c2 = c2_retried / max(max_free, 1e-12)
        
        print("\n--- 1. Synthetic Track (comp0) Activations ---")
        for slot in range(14):
            role_label = ""
            if 0 <= slot <= 3: role_label = "speech"
            elif 4 <= slot <= 6: role_label = "drums"
            elif slot == 7: role_label = "sung C0 (routed)"
            elif slot == 8: role_label = "sung C1 (soak)"
            elif slot == 9: role_label = "sung C2 (candidate)"
            else: role_label = "free"
            print(f"Slot {slot:2d} [{role_label:18s}]: {mean_h[slot]:.6f}")
            
        print("\n--- Synthetic Ratios ---")
        print(f"R1 (drums leak)            = max(drums 4-6) / max(free) = {r1_drums*100:.2f}% (Limit: < 8.00%)")
        print(f"R2 (sung C0 leak)          = c0(7) / max(free)          = {r2_c0*100:.2f}% (Limit: < 8.00%)")
        print(f"R_soak (sung C1 soak)      = c1(8) / max(free)          = {r_soak*100:.2f}% (INFO)")
        print(f"R_c2 (sung C2 retried leak)= c2(9) / max(free)          = {r_c2*100:.2f}% (Candidate routing threshold: < 8.00%)")

    # --------------------------------------------------
    # 2. SYNTHETIC SUSTAINED STRINGS
    # --------------------------------------------------
    print("\n--- 2. Synthetic Sustained Strings Fixture ---")
    syn_strings = generate_synthetic_strings(duration_sec=30.0, sr=48000)
    V_strings = compute_stft_mel(syn_strings, mel_matrix)
    H_strings = run_nmfd_h_only(V_strings, W14, n_iter=12)
    mean_hs = np.mean(H_strings, axis=1)
    
    max_free_s = np.max(mean_hs[10:14])
    r2_strings_c0 = mean_hs[7] / max(max_free_s, 1e-12)
    r2_strings_c2 = mean_hs[9] / max(max_free_s, 1e-12)
    
    print(f"C0 Activation on Strings = {mean_hs[7]:.6f} (R2 vs free = {r2_strings_c0*100:.2f}%)")
    print(f"C1 Activation on Strings = {mean_hs[8]:.6f}")
    print(f"C2 Activation on Strings = {mean_hs[9]:.6f} (R_c2 vs free = {r2_strings_c2*100:.2f}%)")

    # --------------------------------------------------
    # 3. ACOUSTIC / VOCAL PRESERVATION (am_contra_30s.wav)
    # --------------------------------------------------
    ac_path = "lineos/m1/sp314-dsp/tests/fixtures/am_contra_30s.wav"
    if os.path.exists(ac_path):
        y_ac, sr_ac = librosa.load(ac_path, sr=48000, mono=True)
        V_ac = compute_stft_mel(y_ac, mel_matrix)
        H_ac = run_nmfd_h_only(V_ac, W14, n_iter=12)
        mean_h_ac = np.mean(H_ac, axis=1)
        
        print("\n--- 3. Acoustic Vocal Track (am_contra) Activations ---")
        for slot in range(14):
            print(f"Slot {slot:2d}: {mean_h_ac[slot]:.6f}")
            
        bass_theft_act = np.max(mean_h_ac[10:14])
        print(f"Bass Theft / Free Slot Max Activation: {bass_theft_act:.6f}")

    # --------------------------------------------------
    # 4. SARAGA NON-WESTERN BENCH (LOCAL INFO)
    # --------------------------------------------------
    saraga_vocal_path = "/tmp/bench_cuts/saraga_vocal_60s.wav"
    saraga_instr_path = "/tmp/bench_cuts/saraga_instr_60s.wav"
    
    if os.path.exists(saraga_vocal_path) and os.path.exists(saraga_instr_path):
        print("\n--- 4. Saraga Bench Cuts (Local INFO) ---")
        y_sv, _ = librosa.load(saraga_vocal_path, sr=48000, mono=True)
        V_sv = compute_stft_mel(y_sv, mel_matrix)
        H_sv = run_nmfd_h_only(V_sv, W14, n_iter=12)
        mean_sv = np.mean(H_sv, axis=1)
        
        y_si, _ = librosa.load(saraga_instr_path, sr=48000, mono=True)
        V_si = compute_stft_mel(y_si, mel_matrix)
        H_si = run_nmfd_h_only(V_si, W14, n_iter=12)
        mean_si = np.mean(H_si, axis=1)
        
        print(f"Saraga Vocal  C0 (slot 7) = {mean_sv[7]:.6f} | C2 (slot 9) = {mean_sv[9]:.6f}")
        print(f"Saraga Instr  C0 (slot 7) = {mean_si[7]:.6f} | C2 (slot 9) = {mean_si[9]:.6f}")

if __name__ == "__main__":
    base_dir = os.path.dirname(__file__)
    v5_path = os.path.join(base_dir, "w_sung_v5.bin")
    v6_path = os.path.join(base_dir, "w_sung_v6.bin")
    if not os.path.exists(v6_path):
        v6_path = os.path.join(os.getcwd(), "w_sung_v6.bin")
    evaluate_w_sung(v5_path, name="w_sung_v5 (Baseline)")
    evaluate_w_sung(v6_path, name="w_sung_v6 (Candidate)")

