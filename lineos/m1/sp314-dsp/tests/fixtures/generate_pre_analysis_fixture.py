#!/usr/bin/env python3
"""
generate_pre_analysis_fixture.py — Python Oracle for PreAnalysisData
Authority: Pre-Analysis Constitution v1.3 §3-§5
Workflow:  Oracle-TDD Step 1 (Python → JSON)

Generates pre_analysis_reference.json with 3 test signals.
All features computed via NumPy/SciPy — no Rust dependency.
"""
import numpy as np
import json, os
from scipy.signal import butter, sosfilt, lfilter

# ── Constants (matching Rust sp314-dsp) ─────────────────────────────
SR = 48000
FFT_SIZE = 2048
HOP_SIZE = 512
N_BINS = FFT_SIZE // 2 + 1
DURATION = 3.0

# Band edges (Constitution §4.4 — 6 bands)
# 8-band edges — matches Rust BAND_EDGES in
# sp314-dsp/src/analysis/pre_analysis.rs.
# Splits old Mid (500-2000Hz) and HighMid
# (2-8kHz) for surgical speech correction.
BAND_EDGES = [20, 80, 250, 500, 1000, 2000, 4000, 8000, 20000]

# Zone thresholds (Constitution §5)
ZONE_SUB_RUMBLE_DB   = -30.0
ZONE_HARSH_RMS_DB    = -18.0
ZONE_HARSH_CREST_DB  = 8.0
ZONE_BOX_RMS_DB      = -20.0
ZONE_BOX_LRA_LU      = 5.0
ZONE_PHASE_CORR      = 0.3

# Resonant peak detection
PEAK_WIN = 20       # ±20 bins
PEAK_SIGMA = 3.0
MAX_PEAKS = 16

# ── K-weighting (ITU-R BS.1770-4, 48kHz coefficients) ──────────────
def k_weight(sig):
    b1 = [1.53512485958697, -2.69169618940638, 1.19839281085285]
    a1 = [1.0, -1.69065929318241, 0.73248077421585]
    b2 = [1.0, -2.0, 1.0]
    a2 = [1.0, -1.99004745483398, 0.99007225036621]
    return lfilter(b2, a2, lfilter(b1, a1, sig))

# ── Feature computations ───────────────────────────────────────────

def compute_integrated_lufs(left, right):
    kl, kr = k_weight(left), k_weight(right)
    block = int(0.4 * SR)
    hop = int(0.1 * SR)
    powers, pos = [], 0
    while pos + block <= len(kl):
        p = 0.5 * (np.mean(kl[pos:pos+block]**2) + np.mean(kr[pos:pos+block]**2))
        if p > 1e-20: powers.append(p)
        pos += hop
    if not powers: return -144.0
    powers = np.array(powers)
    lufs_b = -0.691 + 10*np.log10(powers+1e-30)
    # Absolute gate -70 LUFS
    ag = lufs_b > -70.0
    if not np.any(ag): return -144.0
    ag_mean = -0.691 + 10*np.log10(np.mean(powers[ag])+1e-30)
    # Relative gate -10 LU
    rg = lufs_b > (ag_mean - 10.0)
    if not np.any(rg): return float(ag_mean)
    return float(-0.691 + 10*np.log10(np.mean(powers[rg])+1e-30))

def compute_true_peak(signal, fir_path):
    with open(fir_path) as f:
        fir = json.load(f)
    h = np.array(fir['minimum_fir']['coefficients'], dtype=np.float64)
    sig64 = signal.astype(np.float64)
    up = np.zeros(len(sig64) * 4)
    up[::4] = sig64 * 4.0
    interp = lfilter(h, 1.0, up)
    tail = interp[len(h):]
    if len(tail) == 0: return -144.0
    pk = float(np.max(np.abs(tail)))
    return -144.0 if pk < 1e-30 else float(20*np.log10(pk))

def compute_lra(left, right):
    kl, kr = k_weight(left), k_weight(right)
    block = int(0.4 * SR)
    hop = int(0.1 * SR)
    bl_lufs, pos = [], 0
    while pos + block <= len(kl):
        p = 0.5 * (np.mean(kl[pos:pos+block]**2) + np.mean(kr[pos:pos+block]**2))
        if p > 1e-20:
            bl_lufs.append(-0.691 + 10*np.log10(p))
        pos += hop
    if len(bl_lufs) < 2: return 0.0
    bl = np.array(bl_lufs)
    ag = bl[bl > -70.0]
    if len(ag) < 2: return 0.0
    ag_mean = -0.691 + 10*np.log10(np.mean(10**((ag+0.691)/10))+1e-30)
    rg = ag[ag > ag_mean - 20.0]
    if len(rg) < 2: return 0.0
    return float(np.percentile(rg, 95) - np.percentile(rg, 10))

def compute_dynamic_range(left):
    block = int(0.1 * SR)  # 100ms blocks
    rms_vals = []
    for i in range(0, len(left) - block, block):
        ms = np.mean(left[i:i+block]**2)
        if ms > 1e-30:
            rms_vals.append(10*np.log10(ms))
    if len(rms_vals) < 2: return 0.0
    rms_vals.sort()
    n = len(rms_vals)
    return float(rms_vals[min(n*95//100, n-1)] - rms_vals[min(n*5//100, n-1)])

def compute_crest_factor(left, right):
    mono = 0.5*(left + right)
    peak = np.max(np.abs(mono))
    rms = np.sqrt(np.mean(mono**2))
    if rms < 1e-10: return 0.0
    return float(20*np.log10(peak/rms))

def compute_spectral_profile(left, right):
    """6-band absolute RMS in dBFS (Constitution §4.4)."""
    nyq = SR / 2.0
    bands = []
    for i in range(8):
        lo = max(BAND_EDGES[i] / nyq, 0.001)
        hi = min(BAND_EDGES[i+1] / nyq, 0.999)
        if lo >= hi:
            bands.append(-144.0); continue
        sos = butter(4, [lo, hi], btype='bandpass', output='sos')
        fl = sosfilt(sos, left)
        fr = sosfilt(sos, right)
        rms = np.sqrt(0.5*(np.mean(fl**2) + np.mean(fr**2)))
        bands.append(-144.0 if rms < 1e-20 else float(20*np.log10(rms)))
    return bands


def compute_transient_density(left, right):
    """Dual MA envelope, +6dB threshold, events/sec (Constitution §4.6)."""
    mono = np.abs(0.5*(left + right))
    fast_win = max(1, int(0.010 * SR))  # 10ms
    slow_win = max(1, int(0.100 * SR))  # 100ms
    # Moving averages via cumsum
    def ma(x, w):
        cs = np.cumsum(x)
        cs = np.insert(cs, 0, 0)
        out = (cs[w:] - cs[:-w]) / w
        # Pad to same length
        return np.concatenate([np.full(w-1, out[0] if len(out) else 0), out])
    fast = ma(mono, fast_win)
    slow = ma(mono, slow_win)
    n = min(len(fast), len(slow))
    fast, slow = fast[:n], slow[:n]
    threshold_linear = 10**(6.0/20.0)  # +6dB = ~2.0x
    mask = fast > slow * threshold_linear
    # Count False→True transitions (leading edges)
    transitions = np.diff(mask.astype(np.int8))
    count = int(np.sum(transitions == 1))
    duration = n / SR
    return float(count / duration) if duration > 0 else 0.0

def compute_phase_correlation(left, right):
    cross = np.sum(left * right)
    sl = np.sum(left**2)
    sr_ = np.sum(right**2)
    denom = np.sqrt(sl * sr_)
    if denom < 1e-10: return 1.0
    return float(np.clip(cross / denom, -1.0, 1.0))


def compute_zone_flags(profile, crest, lra, corr, peaks):
    return {
        "zone_cymbal_harsh":    bool(profile[5] > ZONE_HARSH_RMS_DB and crest < ZONE_HARSH_CREST_DB),
        "zone_sub_rumble":      bool(profile[0] > ZONE_SUB_RUMBLE_DB),
        "zone_boxiness":        bool(profile[2] > ZONE_BOX_RMS_DB and lra < ZONE_BOX_LRA_LU),
        "zone_phase_issue":     bool(corr < ZONE_PHASE_CORR),
        "zone_harsh_resonance": False,
    }

# ── Signal generators ──────────────────────────────────────────────

def gen_sine_1000_mono():
    t = np.arange(int(DURATION * SR), dtype=np.float64) / SR
    sig = (0.5 * np.sin(2 * np.pi * 1000.0 * t)).astype(np.float32)
    return sig, sig.copy()  # L = R (mono)

def gen_white_noise_decorr():
    rng = np.random.default_rng(seed=42)
    n = int(DURATION * SR)
    left  = (rng.standard_normal(n) * 0.3).astype(np.float32)
    right = (rng.standard_normal(n) * 0.3).astype(np.float32)
    return left, right

def gen_problem_mix():
    """
    Designed to trigger: sub_rumble, boxiness, cymbal_harsh, phase_issue.
    - Sub rumble: strong 60Hz → band[0] > -30 dBFS
    - Boxiness: strong 350Hz → band[2] > -20 dBFS, LRA < 5
    - Cymbal harsh: dense 3kHz cluster → band[4] > -18 dBFS, crest < 8
    - Phase issue: anti-phase noise → correlation < 0.3
    """
    rng = np.random.default_rng(seed=99)
    t = np.arange(int(DURATION * SR), dtype=np.float64) / SR
    n = len(t)
    # Sub rumble (60Hz, very loud)
    sub = 0.7 * np.sin(2 * np.pi * 60.0 * t)
    # Boxiness (350Hz, loud — center of 250-500Hz band)
    box = 0.7 * np.sin(2 * np.pi * 350.0 * t)
    box += 0.3 * np.sin(2 * np.pi * 400.0 * t)
    # Dense harsh cluster (3-5kHz, many partials → low crest)
    harsh = np.zeros_like(t)
    for freq in [2500, 3000, 3500, 4000, 4500, 5000, 5500, 6000]:
        harsh += 0.2 * np.sin(2 * np.pi * freq * t + rng.uniform(0, 2*np.pi))
    # Heavy decorrelated noise (pushes correlation below 0.3)
    noise_l = rng.standard_normal(n) * 0.4
    noise_r = rng.standard_normal(n) * 0.4
    # Build L/R with different balances to decorrelate
    left  = (sub + box + harsh + noise_l).astype(np.float64)
    right = (-sub*0.5 + box*0.3 + harsh*0.6 + noise_r).astype(np.float64)
    # Hard clip to crush crest factor below 8dB
    left  = np.clip(left,  -0.9, 0.9)
    right = np.clip(right, -0.9, 0.9)
    return left.astype(np.float32), right.astype(np.float32)



# ── Analyze one signal ─────────────────────────────────────────────

def analyze(signal_id, left, right, fir_path):
    left64  = left.astype(np.float64)
    right64 = right.astype(np.float64)

    lufs     = compute_integrated_lufs(left64, right64)
    tp_l     = compute_true_peak(left, fir_path)
    tp_r     = compute_true_peak(right, fir_path)
    tp       = max(tp_l, tp_r)
    lra      = compute_lra(left64, right64)
    dyn      = compute_dynamic_range(left64)
    crest    = compute_crest_factor(left64, right64)
    profile  = compute_spectral_profile(left64, right64)
    td       = compute_transient_density(left64, right64)
    corr     = compute_phase_correlation(left64, right64)
    flags    = compute_zone_flags(profile, crest, lra, corr, [])

    return {
        "signal_id":                signal_id,
        "sample_rate":              SR,
        "duration_ms":              float(len(left) / SR * 1000.0),
        "channel_count":            2,
        "integrated_lufs":          round(lufs, 4),
        "true_peak_dbtp":           round(tp, 4),
        "loudness_range":           round(lra, 4),
        "dynamic_range_db":         round(dyn, 4),
        "global_crest_factor_db":   round(crest, 4),
        "spectral_profile_db":      [round(x, 4) for x in profile],
        "transient_density":        round(td, 4),
        "global_phase_correlation": round(corr, 6),
        "zone_flags":               flags,
        "tolerances": {
            "integrated_lufs": 1.0,
            "true_peak_dbtp": 0.5,
            "loudness_range": 1.0,
            "dynamic_range_db": 1.0,
            "global_crest_factor_db": 0.5,
            "spectral_profile_db": 2.0,
            "transient_density": 1.0,
            "global_phase_correlation": 0.05,
        },
    }

# ── Main ───────────────────────────────────────────────────────────

def main():
    fir_path = os.path.join(os.path.dirname(__file__), "true_peak_fir.json")
    if not os.path.exists(fir_path):
        raise FileNotFoundError(f"Required: {fir_path}")

    signals = [
        ("sine_1000_mono",           gen_sine_1000_mono),
        ("white_noise_decorrelated", gen_white_noise_decorr),
        ("problem_mix",              gen_problem_mix),
    ]

    results = []
    for sid, gen in signals:
        print(f"Analyzing: {sid}...")
        left, right = gen()
        result = analyze(sid, left, right, fir_path)
        results.append(result)
        print(f"  LUFS={result['integrated_lufs']:.1f}  "
              f"corr={result['global_phase_correlation']:.3f}  "
              f"td={result['transient_density']:.1f}/s  "
              f"zones={result['zone_flags']}")

    out_path = os.path.join(os.path.dirname(__file__),
                            "pre_analysis_reference.json")
    with open(out_path, "w") as f:
        json.dump(results, f, indent=2)
    print(f"\nWritten: {out_path}")
    print(f"  {len(results)} test cases")

if __name__ == "__main__":
    main()
