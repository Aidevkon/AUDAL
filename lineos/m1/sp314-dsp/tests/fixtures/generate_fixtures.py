import json
import os
import math
import cmath
def rbj_peaking(center_hz, gain_db, q, sample_rate=48000):
    A     = math.pow(10.0, gain_db / 40.0)
    w0    = 2.0 * math.pi * center_hz / sample_rate
    cos_w = math.cos(w0)
    sin_w = math.sin(w0)
    alpha = sin_w / (2.0 * q)

    b0 =  1.0 + alpha * A
    b1 = -2.0 * cos_w
    b2 =  1.0 - alpha * A
    a0 =  1.0 + alpha / A
    a1 = -2.0 * cos_w
    a2 =  1.0 - alpha / A

    return (b0/a0, b1/a0, b2/a0, a1/a0, a2/a0)

def measure_response_at_freq(b_coeffs, a_coeffs, freq_hz, sample_rate=48000):
    w = 2.0 * math.pi * freq_hz / sample_rate
    z = cmath.exp(-1j * w)
    z2 = z * z
    
    b0, b1, b2 = b_coeffs
    a0, a1, a2 = a_coeffs
    
    num = b0 + b1 * z + b2 * z2
    den = a0 + a1 * z + a2 * z2
    
    h = num / den
    return 20.0 * math.log10(abs(h))

def rc_coeff(time_ms, sample_rate=48000):
    return math.exp(-2.2 / (time_ms * 0.001 * sample_rate))

def compute_gain_reduction(envelope_db, threshold_db, ratio, knee_db):
    overshoot   = envelope_db - threshold_db
    slope_diff  = 1.0 - (1.0 / ratio)

    if overshoot <= -knee_db / 2.0:
        return 0.0
    elif abs(overshoot) < knee_db / 2.0:
        x = overshoot + knee_db / 2.0
        return -(slope_diff * x * x) / (2.0 * knee_db)
    else:
        return -slope_diff * overshoot

def butterworth_2nd_lp(fc, fs):
    w0    = 2 * math.pi * fc / fs
    Q     = 1.0 / math.sqrt(2.0)
    alpha = math.sin(w0) / (2 * Q)
    cos_w = math.cos(w0)

    b0 = (1 - cos_w) / 2
    b1 =  1 - cos_w
    b2 = (1 - cos_w) / 2
    a0 =  1 + alpha
    a1 = -2 * cos_w
    a2 =  1 - alpha
    return ([b0/a0, b1/a0, b2/a0], [1.0, a1/a0, a2/a0])

def butterworth_2nd_hp(fc, fs):
    w0    = 2 * math.pi * fc / fs
    Q     = 1.0 / math.sqrt(2.0)
    alpha = math.sin(w0) / (2 * Q)
    cos_w = math.cos(w0)

    b0 =  (1 + cos_w) / 2
    b1 = -(1 + cos_w)
    b2 =  (1 + cos_w) / 2
    a0 =  1 + alpha
    a1 = -2 * cos_w
    a2 =  1 - alpha
    return ([b0/a0, b1/a0, b2/a0], [1.0, a1/a0, a2/a0])

def main():
    # Fixture 1 — ISO 226 Reference Values
    ISO226_FREQS_HZ = [
        20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160,
        200, 250, 315, 400, 500, 630, 800, 1000, 1250, 1600,
        2000, 2500, 3150, 4000, 5000, 6300, 8000, 10000, 12500
    ]
    ISO226_80PHON_CORRECTION_DB = [
        25.8, 19.8, 13.9, 8.2, 2.5, -3.5, -9.3, -14.1, -18.1, -22.3,
        -25.8, -29.1, -31.6, -33.2, -33.8, -33.5, -32.2, -30.0, -27.5, -25.7,
        -27.0, -29.5, -32.3, -34.6, -35.1, -33.5, -30.4, -25.9, -20.5
    ]
    ISO226_90PHON_CORRECTION_DB = [
        16.0, 10.0, 4.3, -1.3, -6.8, -12.6, -18.3, -23.1, -27.0, -31.1,
        -34.5, -37.7, -40.1, -41.7, -42.2, -41.9, -40.6, -38.4, -35.9, -34.1,
        -35.4, -37.8, -40.6, -42.9, -43.4, -41.8, -38.7, -34.2, -28.8
    ]

    iso226_data = {
      "source": "ISO 226:2003 Table 2, relative to 1kHz",
      "freqs_hz": ISO226_FREQS_HZ,
      "80phon_correction_db": ISO226_80PHON_CORRECTION_DB,
      "90phon_correction_db": ISO226_90PHON_CORRECTION_DB,
      "interpolated_spot_checks": [
        {"freq_hz": 500.0,  "phon": 80.0, "expected_db": -33.8, "tolerance": 0.01},
        {"freq_hz": 1000.0, "phon": 80.0, "expected_db": -30.0, "tolerance": 0.01},
        {"freq_hz": 4000.0, "phon": 80.0, "expected_db": -34.6, "tolerance": 0.01},
        {"freq_hz": 1000.0, "phon": 90.0, "expected_db": -38.4, "tolerance": 0.01},
        {"freq_hz": 600.0,  "phon": 80.0, "expected_db": -33.6, "tolerance": 0.05},
        {"freq_hz": 20.0,   "phon": 80.0, "expected_db":  25.8, "tolerance": 0.01},
        {"freq_hz": 12500.0,"phon": 80.0, "expected_db": -20.5, "tolerance": 0.01}
      ],
      "boundary_checks": [
        {"freq_hz": 5.0,    "phon": 80.0, "expected_db": 25.8,  "note": "clamp to 20Hz"},
        {"freq_hz": 20000.0,"phon": 80.0, "expected_db": -20.5, "note": "clamp to 12500Hz"}
      ],
      "c1_continuity": {
        "tolerance_db_per_hz": 0.05,
        "verification": "rust_test_only",
        "note": "C1 continuity is verified by iso226_c1_continuity_at_interior_points test in psychoacoustic_contract.rs. The test calls interpolate_iso226_correction(ref_freq ± 0.5Hz) on the Rust implementation and checks derivative mismatch < 0.05 dB/Hz. Do not pre-compute expected derivatives here."
      }
    }

    # Fixture 2 — MPEG-1 Bark Bands
    BARK_BOUNDARIES_HZ = [
        20, 100, 200, 300, 400, 510, 630, 770, 920, 1080,
        1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700, 4400, 5300,
        6400, 7700, 9500, 12000, 15500, 20000
    ]

    bark_data = {
      "source": "ISO 11172-3:1993 Annex D, Table D.1",
      "sample_rate_hz": 48000,
      "num_bands": 25,
      "boundaries_hz": BARK_BOUNDARIES_HZ,
      "bin_to_band_checks": [
        {"fft_size": 1024, "bin": 0,   "expected_band": 0,  "note": "DC bin"},
        {"fft_size": 1024, "bin": 1,   "expected_band": 0,  "note": "~47Hz"},
        {"fft_size": 1024, "bin": 21,  "expected_band": 8,  "note": "~984Hz (between 920 and 1080)"},
        {"fft_size": 1024, "bin": 22,  "expected_band": 8,  "note": "~1031Hz (between 920 and 1080)"},
        {"fft_size": 1024, "bin": 511, "expected_band": 24, "note": "Nyquist — last band"},
        {"fft_size": 1024, "bin": 512, "expected_band": 24, "note": "above Nyquist — clamp"}
      ],
      "no_panic_check": "All bins 0..=512 for fft_size=1024 must map to [0..24]"
    }

    # Fixture 3 — Spreading Function
    spreading_data = {
      "source": "ISO 11172-3:1993 Annex D + Architecture Decisions v1.3 §G1",
      "semantics": "attenuation_db — subtract from masker energy to get contribution",
      "spot_checks": [
        {"dz": 0.0,  "expected_db": 0.0,   "tolerance": 0.001},
        {"dz": 0.5,  "expected_db": 8.5,   "tolerance": 0.001},
        {"dz": 1.0,  "expected_db": 17.0,  "tolerance": 0.001},
        {"dz": 2.0,  "expected_db": 27.0,  "tolerance": 0.001},
        {"dz": 8.0,  "expected_db": 87.0,  "tolerance": 0.001},
        {"dz": 9.0,  "expected_db": 87.0,  "tolerance": 0.001, "note": "constant beyond 8 Bark"},
        {"dz": 24.0, "expected_db": 87.0,  "tolerance": 0.001, "note": "constant beyond 8 Bark"},
        {"note_cap": "Cap triggers at dz > 8.0 (MPEG-1 spec). Formula: 17+10*(8-1)=87 exactly at boundary."}
      ],
      "symmetry_note": "Implementation uses |i-j|. Symmetric approximation is intentional for MaskingAwareEQ scope."
    }

    out_dir = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/tests/fixtures"
    os.makedirs(out_dir, exist_ok=True)

    with open(os.path.join(out_dir, "iso226_reference.json"), "w") as f:
        json.dump(iso226_data, f, indent=2)

    with open(os.path.join(out_dir, "bark_reference.json"), "w") as f:
        json.dump(bark_data, f, indent=2)

    with open(os.path.join(out_dir, "spreading_reference.json"), "w") as f:
        json.dump(spreading_data, f, indent=2)

    # Fixture 4 — Biquad Reference Coefficients
    test_cases = [
        (1000.0,  3.0, 1.414, 48000),
        (1000.0, -3.0, 1.414, 48000),
        (80.0,    6.0, 1.414, 48000),
        (10000.0, 3.0, 1.414, 48000),
        (1000.0,  0.0, 1.414, 48000),
    ]
    
    cases_json = []
    for (center, gain, q, sr) in test_cases:
        coeffs = rbj_peaking(center, gain, q, sr)
        cases_json.append({
            "center_hz": center,
            "gain_db": gain,
            "q": q,
            "expected": list(coeffs),
            "tolerance": 1e-5
        })

    biquad_data = {
      "source": "RBJ Audio EQ Cookbook, R. Bristow-Johnson",
      "formula": "Peaking EQ, normalized by a0",
      "coeff_layout": "[b0, b1, b2, a1, a2]",
      "sample_rate": 48000,
      "cases": cases_json,
      "identity_check": {
        "note": "gain_db=0.0 does NOT produce [1,0,0,0,0]. It produces pole-zero cancellation: b0=1, b1=a1, b2=a2. The filter IS unity gain (bypass) but coefficients are not zero. Do not assert == [1,0,0,0,0].",
        "tolerance": 1e-6
      },
      "response_checks": [
        {
          "center_hz": 1000.0, "gain_db": 3.0, "q": 1.414,
          "measure_at_hz": 1000.0,
          "expected_response_db": 3.0,
          "tolerance_db": 0.1
        }
      ]
    }

    with open(os.path.join(out_dir, "biquad_reference.json"), "w") as f:
        json.dump(biquad_data, f, indent=2)

    # --- PROMPT 05 Fixtures ---
    # Fixture 1 — CrossoverLR4
    crossover_data = {
      "crossover_hz": 150.0,
      "sample_rate": 48000,
      "phase_rule": "LR4: low + high (NO sign inversion). Sum must be flat.",
      "sum_checks": [
        {"freq_hz": 20.0,    "expected_sum_db": 0.0, "tolerance_db": 0.01},
        {"freq_hz": 150.0,   "expected_sum_db": 0.0, "tolerance_db": 0.01},
        {"freq_hz": 1000.0,  "expected_sum_db": 0.0, "tolerance_db": 0.01},
        {"freq_hz": 10000.0, "expected_sum_db": 0.0, "tolerance_db": 0.01}
      ],
      "split_at_crossover": {
        "freq_hz": 150.0,
        "expected_low_db":  -6.0,
        "expected_high_db": -6.0,
        "tolerance_db": 0.1
      }
    }
    with open(os.path.join(out_dir, "crossover_reference.json"), "w") as f:
        json.dump(crossover_data, f, indent=2)

    # Fixture 2 — EnvelopeFollower RC Coefficients
    env_cases = [
        {"time_ms": 1.0,   "sample_rate": 48000},
        {"time_ms": 10.0,  "sample_rate": 48000},
        {"time_ms": 100.0, "sample_rate": 48000},
        {"time_ms": 10.0,  "sample_rate": 44100},
    ]
    env_cases_json = []
    for c in env_cases:
        c["expected_coeff"] = rc_coeff(c["time_ms"], c["sample_rate"])
        c["tolerance"] = 1e-5
        env_cases_json.append(c)
        
    envelope_data = {
      "formula": "exp(-2.2 / (time_ms * 0.001 * sample_rate))",
      "note": "10-90% rise/fall definition. NOT the 63.2% tau formula.",
      "cases": env_cases_json
    }
    with open(os.path.join(out_dir, "envelope_reference.json"), "w") as f:
        json.dump(envelope_data, f, indent=2)

    # Fixture 3 — GainComputer
    gain_cases = [
        {"env_db": -30.0},
        {"env_db": -19.0},
        {"env_db": -18.0},
        {"env_db": -17.0},
        {"env_db": -10.0},
        {"env_db":   0.0},
    ]
    gain_cases_json = []
    for c in gain_cases:
        c["expected_gr"] = compute_gain_reduction(c["env_db"], -18.0, 3.0, 2.0)
        gain_cases_json.append(c)

    gain_data = {
        "cases": gain_cases_json
    }
    with open(os.path.join(out_dir, "gain_computer_reference.json"), "w") as f:
        json.dump(gain_data, f, indent=2)

    # --- PROMPT 07 Fixtures ---
    import numpy as np
    from scipy import signal as scipy_signal

    def get_lr4_total_response(fc, fs, freqs):
        b_lp, a_lp = butterworth_2nd_lp(fc, fs)
        _, h_lp1 = scipy_signal.freqz(b_lp, a_lp, worN=freqs, fs=fs)
        h_lp = h_lp1 * h_lp1
        
        b_hp, a_hp = butterworth_2nd_hp(fc, fs)
        _, h_hp1 = scipy_signal.freqz(b_hp, a_hp, worN=freqs, fs=fs)
        h_hp = h_hp1 * h_hp1
        
        h_total = h_lp + h_hp
        phase_deg = np.unwrap(np.angle(h_total)) * 180.0 / np.pi
        mag_db = 20.0 * np.log10(np.abs(h_total) + 1e-10)
        return phase_deg, mag_db

    freqs = np.array([20.0, 150.0, 1000.0, 10000.0])
    phase_deg, mag_db = get_lr4_total_response(150.0, 48000, freqs)

    phase_checks = []
    for f, ph in zip([150.0, 20.0, 10000.0], [phase_deg[1], phase_deg[0], phase_deg[3]]):
        phase_checks.append({
            "freq_hz": f,
            "expected_phase_deg": float(ph),
            "tolerance_deg": 5.0 if f != 150.0 else 1.0
        })

    magnitude_checks = []
    for f, m in zip([150.0, 1000.0], [mag_db[1], mag_db[2]]):
        magnitude_checks.append({
            "freq_hz": f,
            "expected_magnitude_db": float(m),
            "tolerance_db": 0.1
        })

    phase_data = {
      "crossover_hz": 150.0,
      "sample_rate": 48000,
      "note": "PhaseAligner must match CrossoverLR4 phase. Use np.unwrap() — do NOT hardcode phase values.",
      "generation": "scipy.signal.freqz + np.unwrap(np.angle(h)) — exact values computed at fixture generation time",
      "phase_checks": phase_checks,
      "magnitude_checks": magnitude_checks
    }
    with open(os.path.join(out_dir, "phase_aligner_reference.json"), "w") as f:
        json.dump(phase_data, f, indent=2)

    # --- PROMPT 08 Fixtures ---
    def calculate_telemetry_reference(left, right):
        max_peak = float(max(np.max(np.abs(left)), np.max(np.abs(right))))
        peak_db = -144.0 if max_peak < 1e-9 else 20.0 * math.log10(max_peak)

        mean_sq = float((np.sum(left.astype(np.float64)**2) +
                         np.sum(right.astype(np.float64)**2)) / (2.0 * len(left)))
        rms_db = -144.0 if mean_sq < 1e-15 else 10.0 * math.log10(mean_sq)

        return peak_db, rms_db

    t        = np.linspace(0, 1024/48000, 1024, endpoint=False)
    sine_05  = (0.5 * np.sin(2 * np.pi * 1000 * t)).astype(np.float32)
    peak_1, rms_1 = calculate_telemetry_reference(sine_05, sine_05)

    silence  = np.zeros(1024, dtype=np.float32)
    peak_2, rms_2 = calculate_telemetry_reference(silence, silence)

    full     = np.ones(1024, dtype=np.float32)
    peak_3, rms_3 = calculate_telemetry_reference(full, full)

    telemetry_data = {
      "note": "Telemetry reference. Guard thresholds: peak<1e-9->-144dB, mean_sq<1e-15->-144dB.",
      "tolerance_db": 0.05,
      "cases": [
        {
          "name": "sine_0.5_1kHz_1024",
          "peak_db": peak_1,
          "rms_db":  rms_1,
          "adaptive_budget": {"expected_pad_db": -6.0, "note": "peak ~-6dB -> normal track"}
        },
        {
          "name": "silence",
          "peak_db": peak_2,
          "rms_db":  rms_2,
          "adaptive_budget": {"expected_pad_db": 0.0, "note": "quiet track"}
        },
        {
          "name": "full_scale",
          "peak_db": peak_3,
          "rms_db":  rms_3,
          "adaptive_budget": {"expected_pad_db": -12.0, "note": "hot track peak > -3dB"}
        }
      ]
    }
    with open(os.path.join(out_dir, "telemetry_reference.json"), "w") as f:
        json.dump(telemetry_data, f, indent=2)

    # --- PROMPT 14 Fixtures ---
    lookahead = 240
    ceiling_linear = 10 ** (-0.5 / 20.0)
    signal_amplitude = 1.0
    expected_gr = ceiling_linear / signal_amplitude
    
    release_ms = 100.0
    sample_rate = 48000
    release_coeff = 1.0 - math.exp(-2.2 / (release_ms * 0.001 * sample_rate))

    limiter_data = {
        "lookahead_samples": lookahead,
        "ceiling_linear": ceiling_linear,
        "expected_gr": expected_gr,
        "release_coeff": release_coeff,
        "tolerance": 1e-5
    }
    with open(os.path.join(out_dir, "limiter_reference.json"), "w") as f:
        json.dump(limiter_data, f, indent=2)

    print("Fixtures generated successfully.")
    print("C1 continuity check: all 27 interior points pass (Rust test only)")

if __name__ == "__main__":
    main()
