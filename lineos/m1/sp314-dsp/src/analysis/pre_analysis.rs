// analysis/pre_analysis.rs — PreAnalyzer implementation
// Authority: Pre-Analysis Constitution v1.3
// Workflow:  Oracle-TDD Step 4 (GREEN)
// Rule:      libm only. No std::f32 trig/log. No rand.

use crate::metering::{mean_square_to_lufs, KWeightingFilter};
use crate::stft::StftEngine;
use lineos_types::pre_analysis::*;

// ── Polyphase FIR coefficients (from true_peak_fir.json, Blackman-Harris 71-tap) ──
const POLYPHASE: [[f32; 18]; 4] = [
    // Phase 0
    [
        1.543_402_8e-6,
        -7.130_015e-5,
        5.479_165_7e-4,
        -2.394_334_9e-3,
        7.657_780_3e-3,
        -1.997_707_4e-2,
        4.589_402_3e-2,
        -1.021_004_1e-1,
        2.877_407_4e-1,
        8.961_255e-1,
        -1.601_431_8e-1,
        6.812_955e-2,
        -3.061_716_8e-2,
        1.262_447_2e-2,
        -4.416_339_6e-3,
        1.202_527_2e-3,
        -2.188_774_5e-4,
        1.504_900_6e-5,
    ],
    // Phase 1
    [
        6.598_318e-6,
        -1.826_468_8e-4,
        1.163_958_6e-3,
        -4.638_173e-3,
        1.398_441_4e-2,
        -3.509_535e-2,
        7.909_798_6e-2,
        -1.791_762_1e-1,
        6.248_379_3e-1,
        6.248_379_3e-1,
        -1.791_762_1e-1,
        7.909_798_6e-2,
        -3.509_535e-2,
        1.398_441_4e-2,
        -4.638_173e-3,
        1.163_958_6e-3,
        -1.826_468_8e-4,
        6.598_318e-6,
    ],
    // Phase 2
    [
        1.504_900_6e-5,
        -2.188_774_5e-4,
        1.202_527_2e-3,
        -4.416_339_6e-3,
        1.262_447_2e-2,
        -3.061_716_8e-2,
        6.812_955e-2,
        -1.601_431_8e-1,
        8.961_255e-1,
        2.877_407_4e-1,
        -1.021_004_1e-1,
        4.589_402_3e-2,
        -1.997_707_4e-2,
        7.657_780_3e-3,
        -2.394_334_9e-3,
        5.479_165_7e-4,
        -7.130_015e-5,
        1.543_402_8e-6,
    ],
    // Phase 3 (pass-through)
    [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.000_002_3,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
];

const TAPS_PER_PHASE: usize = 18;
#[allow(dead_code)]
const N_PHASES: usize = 4;

/// 8-band spectral profile crossovers.
/// Splits the old Mid (500-2000Hz) and HighMid
/// (2-8kHz) for surgical speech correction:
///   [3] Mid-Low  500-1000Hz (mud/honk)
///   [4] Mid-High 1000-2000Hz (articulation)
///   [5] HighMid  2000-4000Hz (presence/Byrne)
///   [6] Treble   4000-8000Hz
const BAND_EDGES: [f32; 9] = [
    20.0, 80.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 20000.0,
];

// K-weight block sizes for LRA (48kHz)
const LRA_BLOCK: usize = 19200; // 400ms
const LRA_HOP: usize = 4800; // 100ms

pub struct PreAnalyzer;

impl PreAnalyzer {
    pub fn run(left: &[f32], right: &[f32], sample_rate: u32) -> PreAnalysisData {
        if left.len() < MINIMUM_ANALYSIS_SAMPLES || right.len() < MINIMUM_ANALYSIS_SAMPLES {
            return PreAnalysisData::silent();
        }
        let n = left.len().min(right.len());
        let left = &left[..n];
        let right = &right[..n];

        // ── Group A: reuse existing ──
        let integrated_lufs = crate::metering::measure_integrated_lufs(left, right);

        // Interleave for stereo helpers
        let stereo: Vec<f32> = left
            .iter()
            .zip(right.iter())
            .flat_map(|(&l, &r)| [l, r])
            .collect();
        let global_phase_correlation = crate::analysis::stereo::stereo_correlation(&stereo);
        let stereo_width = (1.0_f32 - global_phase_correlation).clamp(0.0, 1.0);

        let mono: Vec<f32> = left
            .iter()
            .zip(right.iter())
            .map(|(&l, &r)| 0.5 * (l + r))
            .collect();
        let dynamic_range_db = crate::analysis::dynamics::dynamic_range_db(&mono, sample_rate);
        let global_crest_factor_db = crate::analysis::dynamics::crest_factor_db(&mono);

        // ── Group B: new implementations ──
        let true_peak_dbtp = true_peak_detect(left, right);
        let loudness_range = compute_lra(left, right);
        let (spectral_profile_db, band_signals_l, band_signals_r) =
            spectral_profile_8band(left, right, sample_rate);
        let spectral_rolloff_hz = spectral_rolloff_85(left, right, sample_rate);
        let transient_density = compute_transient_density(&mono, sample_rate);
        let side_mid_ratio_db = compute_side_mid_ratio(left, right);
        let band_phase_correlation = band_phase_correlation_8(&band_signals_l, &band_signals_r);
        let resonant_peaks_hz = compute_resonant_peaks(left, right, sample_rate);
        let zone_flags = compute_zone_flags(
            &spectral_profile_db,
            global_crest_factor_db,
            loudness_range,
            global_phase_correlation,
            &resonant_peaks_hz,
        );

        PreAnalysisData {
            integrated_lufs,
            true_peak_dbtp,
            loudness_range,
            dynamic_range_db,
            global_crest_factor_db,
            spectral_profile_db,
            spectral_rolloff_hz,
            transient_density,
            global_phase_correlation,
            side_mid_ratio_db,
            stereo_width,
            band_phase_correlation,
            resonant_peaks_hz,
            zone_flags,
            bpm: 0.0,
            beats_ms: vec![],
            downbeats_ms: vec![],
            transients_ms: vec![],
        }
    }
}

// ── True Peak (4x polyphase FIR) ─────────────────────────────────────────────

fn true_peak_detect(left: &[f32], right: &[f32]) -> f32 {
    let tp_l = true_peak_channel(left);
    let tp_r = true_peak_channel(right);
    let peak = if tp_l > tp_r { tp_l } else { tp_r };
    if peak < 1e-30 {
        -144.0
    } else {
        20.0 * libm::log10f(peak)
    }
}

fn true_peak_channel(signal: &[f32]) -> f32 {
    let n = signal.len();
    if n == 0 {
        return 0.0;
    }
    let mut max_peak: f32 = 0.0;
    // Track max of original samples too
    for &s in signal {
        let a = libm::fabsf(s);
        if a > max_peak {
            max_peak = a;
        }
    }
    // Polyphase interpolation: for each input sample, compute 3 interpolated
    // samples (phases 0,1,2; phase 3 ≈ original sample)
    for phase_idx in 0..3 {
        let h = &POLYPHASE[phase_idx];
        for i in TAPS_PER_PHASE..n {
            let mut acc: f32 = 0.0;
            for j in 0..TAPS_PER_PHASE {
                acc += h[j] * signal[i - j];
            }
            let a = libm::fabsf(acc);
            if a > max_peak {
                max_peak = a;
            }
        }
    }
    max_peak
}

// ── LRA (BS.1770-4) ─────────────────────────────────────────────────────────

fn compute_lra(left: &[f32], right: &[f32]) -> f32 {
    let n = left.len();
    if n < LRA_BLOCK {
        return 0.0;
    }

    // K-weight both channels
    let mut filt_l = KWeightingFilter::new();
    let mut filt_r = KWeightingFilter::new();
    let kl: Vec<f32> = left.iter().map(|&s| filt_l.process(s)).collect();
    let kr: Vec<f32> = right.iter().map(|&s| filt_r.process(s)).collect();

    // Collect block mean squares
    let mut block_ms: Vec<f32> = Vec::new();
    let mut pos = 0;
    while pos + LRA_BLOCK <= n {
        let mut sum = 0.0_f32;
        for i in pos..pos + LRA_BLOCK {
            sum += kl[i] * kl[i] + kr[i] * kr[i];
        }
        let ms = sum / (2.0 * LRA_BLOCK as f32);
        block_ms.push(ms);
        pos += LRA_HOP;
    }
    if block_ms.len() < 2 {
        return 0.0;
    }

    // Absolute gate -70 LUFS
    let abs_thresh = libm::powf(10.0, (-70.0 + 0.691) / 10.0);
    let abs_gated: Vec<f32> = block_ms
        .iter()
        .copied()
        .filter(|&ms| ms >= abs_thresh)
        .collect();
    if abs_gated.len() < 2 {
        return 0.0;
    }

    // Relative gate -20 LU below abs-gated mean
    let abs_mean = abs_gated.iter().sum::<f32>() / abs_gated.len() as f32;
    let abs_mean_lufs = mean_square_to_lufs(abs_mean);
    let rel_thresh = libm::powf(10.0, (abs_mean_lufs - 20.0 + 0.691) / 10.0);
    let mut rel_gated_lufs: Vec<f32> = abs_gated
        .iter()
        .copied()
        .filter(|&ms| ms >= rel_thresh)
        .map(mean_square_to_lufs)
        .collect();
    if rel_gated_lufs.len() < 2 {
        return 0.0;
    }

    rel_gated_lufs.sort_by(|a, b| a.total_cmp(b));
    let n = rel_gated_lufs.len();
    let p95 = rel_gated_lufs[(n * 95 / 100).min(n - 1)];
    let p10 = rel_gated_lufs[(n * 10 / 100).min(n - 1)];
    p95 - p10
}

// ── 6-Band Spectral Profile (Butterworth IIR) ────────────────────────────────

/// 2nd-order Butterworth section (biquad) state
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    w1: f32,
    w2: f32,
}

impl Biquad {
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.w1;
        self.w1 = self.b1 * x - self.a1 * y + self.w2;
        self.w2 = self.b2 * x - self.a2 * y;
        if libm::fabsf(self.w1) < 1e-15 {
            self.w1 = 0.0;
        }
        if libm::fabsf(self.w2) < 1e-15 {
            self.w2 = 0.0;
        }
        y
    }
}

/// Design 2nd-order Butterworth lowpass biquad
fn butter_lp2(freq: f32, sr: f32) -> Biquad {
    let w0 = 2.0 * core::f32::consts::PI * freq / sr;
    let cs = libm::cosf(w0);
    let sn = libm::sinf(w0);
    let alpha = sn / (2.0 * core::f32::consts::SQRT_2); // Q = sqrt(2)/2
    let a0 = 1.0 + alpha;
    Biquad {
        b0: ((1.0 - cs) / 2.0) / a0,
        b1: (1.0 - cs) / a0,
        b2: ((1.0 - cs) / 2.0) / a0,
        a1: (-2.0 * cs) / a0,
        a2: (1.0 - alpha) / a0,
        w1: 0.0,
        w2: 0.0,
    }
}

/// Design 2nd-order Butterworth highpass biquad
fn butter_hp2(freq: f32, sr: f32) -> Biquad {
    let w0 = 2.0 * core::f32::consts::PI * freq / sr;
    let cs = libm::cosf(w0);
    let sn = libm::sinf(w0);
    let alpha = sn / (2.0 * core::f32::consts::SQRT_2);
    let a0 = 1.0 + alpha;
    Biquad {
        b0: ((1.0 + cs) / 2.0) / a0,
        b1: -(1.0 + cs) / a0,
        b2: ((1.0 + cs) / 2.0) / a0,
        a1: (-2.0 * cs) / a0,
        a2: (1.0 - alpha) / a0,
        w1: 0.0,
        w2: 0.0,
    }
}

/// Apply 4th-order bandpass (cascade of 2x HP + 2x LP) to extract a band
fn bandpass_filter(signal: &[f32], lo: f32, hi: f32, sr: f32) -> Vec<f32> {
    let mut hp1 = butter_hp2(lo, sr);
    let mut hp2 = butter_hp2(lo, sr);
    let mut lp1 = butter_lp2(hi, sr);
    let mut lp2 = butter_lp2(hi, sr);
    signal
        .iter()
        .map(|&x| {
            let y = hp1.process(x);
            let y = hp2.process(y);
            let y = lp1.process(y);
            lp2.process(y)
        })
        .collect()
}

/// Public measurement entry point for corpus tooling.
/// Thin wrapper over the production 8-band path — same filters,
/// same math, one truth. Returns levels only (dB per band),
/// discarding the filtered-signal buffers the private fn also
/// produces.
pub fn spectral_profile_levels(left: &[f32], right: &[f32], sr: u32) -> [f32; 8] {
    spectral_profile_8band(left, right, sr).0
}

/// Least-squares linear regression of y = levels_db[k] / 20.0 against
/// x = log10(geometric center of band k), using ONLY bands 1..=6
/// (geometric centers 141.4–5656.9 Hz — the Pestana 2013 100Hz–10kHz
/// window; bands 0 and 7 excluded). Returns the slope in Pestana
/// Table 2 units: log10(linear magnitude) per log10(Hz) — i.e.
/// (dB/20) per decade — the SAME units as GATE_V1 slope bounds.
/// Centers computed from BAND_EDGES via libm::sqrtf(lo*hi); all math
/// libm-only; deterministic 6-point approximation of Pestana's
/// dense-spectrum regression, adequate for the deliberately-loose
/// gate (S-0XX §5).
pub fn spectral_slope(levels_db: &[f32; 8]) -> f32 {
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xx = 0.0;
    let mut sum_xy = 0.0;
    let n = 6.0;

    for k in 1..=6 {
        let lo = BAND_EDGES[k];
        let hi = BAND_EDGES[k + 1];
        let center = libm::sqrtf(lo * hi);
        let x = libm::log10f(center);
        let y = levels_db[k] / 20.0;

        sum_x += x;
        sum_y += y;
        sum_xx += x * x;
        sum_xy += x * y;
    }

    let denominator = n * sum_xx - sum_x * sum_x;
    // Unreachable by construction: x values are fixed distinct constants from BAND_EDGES; kept as pure-math hygiene, not an input-dependent sentinel.
    if libm::fabsf(denominator) < 1e-10 {
        return 0.0;
    }

    (n * sum_xy - sum_x * sum_y) / denominator
}

fn spectral_profile_8band(
    left: &[f32],
    right: &[f32],
    sr: u32,
) -> ([f32; 8], Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let srf = sr as f32;
    let nyq = srf / 2.0;
    let mut profile = [-144.0_f32; 8];
    let mut bands_l = Vec::with_capacity(8);
    let mut bands_r = Vec::with_capacity(8);

    for i in 0..8 {
        let lo = BAND_EDGES[i].max(1.0);
        let hi = BAND_EDGES[i + 1].min(nyq - 1.0);
        if lo >= hi {
            bands_l.push(vec![0.0; left.len()]);
            bands_r.push(vec![0.0; right.len()]);
            continue;
        }
        let fl = bandpass_filter(left, lo, hi, srf);
        let fr = bandpass_filter(right, lo, hi, srf);
        let sum_sq: f32 = fl.iter().zip(fr.iter()).map(|(&l, &r)| l * l + r * r).sum();
        let rms = libm::sqrtf(sum_sq / (2.0 * left.len() as f32));
        if rms > 1e-20 {
            profile[i] = 20.0 * libm::log10f(rms);
        }
        bands_l.push(fl);
        bands_r.push(fr);
    }
    (profile, bands_l, bands_r)
}

// ── Spectral Rolloff (85%) ──────────────────────────────────────────────────

fn spectral_rolloff_85(left: &[f32], right: &[f32], sr: u32) -> f32 {
    let mono: Vec<f32> = left
        .iter()
        .zip(right.iter())
        .map(|(&l, &r)| 0.5 * (l + r))
        .collect();
    let mut engine = StftEngine::new();
    let (frames, n_frames) = engine.forward(&mono);
    if n_frames == 0 {
        return 0.0;
    }

    let n_bins = frames[0].len();
    let mut avg_mag = vec![0.0_f32; n_bins];
    for frame in &frames {
        for (i, c) in frame.iter().enumerate() {
            avg_mag[i] += libm::sqrtf(c.re * c.re + c.im * c.im);
        }
    }
    for v in avg_mag.iter_mut() {
        *v /= n_frames as f32;
    }

    let mut cumsum = 0.0_f32;
    let total: f32 = avg_mag.iter().sum();
    if total < 1e-20 {
        return 0.0;
    }
    let target = 0.85 * total;
    for (i, &m) in avg_mag.iter().enumerate() {
        cumsum += m;
        if cumsum >= target {
            return i as f32 * sr as f32 / crate::stft::FFT_SIZE as f32;
        }
    }
    sr as f32 / 2.0
}

// ── Transient Density ───────────────────────────────────────────────────────

fn compute_transient_density(mono: &[f32], sample_rate: u32) -> f32 {
    let n = mono.len();
    if n < 2 {
        return 0.0;
    }
    let fast_win = ((0.010 * sample_rate as f64) as usize).max(1);
    let slow_win = ((0.100 * sample_rate as f64) as usize).max(1);
    let threshold_linear: f32 = libm::powf(10.0, 6.0 / 20.0); // +6dB ≈ 2.0

    // Compute rectified signal
    let rect: Vec<f32> = mono.iter().map(|&s| libm::fabsf(s)).collect();

    // Moving averages via prefix sum
    let mut prefix = vec![0.0_f32; n + 1];
    for i in 0..n {
        prefix[i + 1] = prefix[i] + rect[i];
    }

    let ma = |pos: usize, win: usize| -> f32 {
        if pos < win {
            return prefix[pos + 1] / (pos + 1) as f32;
        }
        (prefix[pos + 1] - prefix[pos + 1 - win]) / win as f32
    };

    // Count leading edges (false→true transitions)
    let mut count = 0u32;
    let mut was_above = false;
    for i in slow_win..n {
        let fast = ma(i, fast_win);
        let slow = ma(i, slow_win);
        let is_above = fast > slow * threshold_linear;
        if is_above && !was_above {
            count += 1;
        }
        was_above = is_above;
    }
    let duration = (n - slow_win) as f32 / sample_rate as f32;
    if duration <= 0.0 {
        return 0.0;
    }
    count as f32 / duration
}

// ── Side/Mid Ratio ──────────────────────────────────────────────────────────

fn compute_side_mid_ratio(left: &[f32], right: &[f32]) -> f32 {
    let mut sum_m2 = 0.0_f32;
    let mut sum_s2 = 0.0_f32;
    for (&l, &r) in left.iter().zip(right.iter()) {
        let m = 0.5 * (l + r);
        let s = 0.5 * (l - r);
        sum_m2 += m * m;
        sum_s2 += s * s;
    }
    let n = left.len() as f32;
    let rms_m = libm::sqrtf(sum_m2 / n);
    let rms_s = libm::sqrtf(sum_s2 / n);
    if rms_m < 1e-20 {
        return -60.0;
    }
    let ratio = 20.0 * libm::log10f(rms_s / rms_m + 1e-30);
    ratio.clamp(-60.0, 6.0)
}

// ── Per-Band Phase Correlation ──────────────────────────────────────────────

fn phase_corr(left: &[f32], right: &[f32]) -> f32 {
    let mut cross = 0.0_f32;
    let mut sl = 0.0_f32;
    let mut sr = 0.0_f32;
    for (&l, &r) in left.iter().zip(right.iter()) {
        cross += l * r;
        sl += l * l;
        sr += r * r;
    }
    let denom = libm::sqrtf(sl * sr);
    if denom < 1e-10 {
        return 1.0;
    }
    (cross / denom).clamp(-1.0, 1.0)
}

fn band_phase_correlation_8(bands_l: &[Vec<f32>], bands_r: &[Vec<f32>]) -> [f32; 8] {
    let mut corrs = [1.0_f32; 8];
    for i in 0..8 {
        corrs[i] = phase_corr(&bands_l[i], &bands_r[i]);
    }
    corrs
}

// ── Resonant Peak Detection ─────────────────────────────────────────────────

fn compute_resonant_peaks(left: &[f32], right: &[f32], sample_rate: u32) -> Vec<f32> {
    let mono: Vec<f32> = left
        .iter()
        .zip(right.iter())
        .map(|(&l, &r)| 0.5 * (l + r))
        .collect();
    let mut engine = StftEngine::new();
    let (frames, n_frames) = engine.forward(&mono);
    if n_frames == 0 {
        return vec![];
    }

    let n_bins = frames[0].len();
    let mut avg_mag = vec![0.0_f32; n_bins];
    for frame in &frames {
        for (i, c) in frame.iter().enumerate() {
            avg_mag[i] += libm::sqrtf(c.re * c.re + c.im * c.im);
        }
    }
    for v in avg_mag.iter_mut() {
        *v /= n_frames as f32;
    }

    // Magnitude floor
    let peak_mag = avg_mag
        .iter()
        .copied()
        .fold(0.0_f32, |a, b| if b > a { b } else { a });
    let mag_floor = peak_mag * RESONANT_PEAK_MAG_FLOOR_RATIO;

    let win = RESONANT_PEAK_WINDOW_BINS;
    let mut raw_peaks: Vec<f32> = Vec::new();

    for b in win..n_bins.saturating_sub(win) {
        if avg_mag[b] < mag_floor {
            continue;
        }

        // Local stats over ±win bins
        let start = b - win;
        let end = (b + win + 1).min(n_bins);
        let local = &avg_mag[start..end];
        let count = local.len() as f32;
        let mu: f32 = local.iter().sum::<f32>() / count;
        let var: f32 = local.iter().map(|&v| (v - mu) * (v - mu)).sum::<f32>() / count;
        let sigma = libm::sqrtf(var);

        if sigma > 1e-15 && avg_mag[b] > mu + RESONANT_PEAK_SIGMA * sigma {
            let hz = b as f32 * sample_rate as f32 / crate::stft::FFT_SIZE as f32;
            raw_peaks.push(hz);
        }
    }

    raw_peaks.sort_by(|a, b| a.total_cmp(b));

    // Thin: keep peaks > 10 Hz apart
    let mut thinned: Vec<f32> = Vec::new();
    for &p in &raw_peaks {
        if thinned.last().is_none_or(|&prev| p - prev > 10.0) {
            thinned.push(p);
        }
        if thinned.len() >= RESONANT_PEAK_MAX_COUNT {
            break;
        }
    }
    thinned
}

// ── Zone Flags ──────────────────────────────────────────────────────────────

fn compute_zone_flags(
    profile: &[f32; 8],
    crest: f32,
    lra: f32,
    corr: f32,
    peaks: &[f32],
) -> ZoneActivationFlags {
    ZoneActivationFlags {
        zone_cymbal_harsh: profile[5] > ZONE_CYMBAL_HARSH_RMS_DB
            && crest < ZONE_CYMBAL_HARSH_CREST_DB,
        zone_sub_rumble: profile[0] > ZONE_SUB_RUMBLE_THRESHOLD_DB,
        zone_boxiness: profile[2] > ZONE_BOXINESS_RMS_DB && lra < ZONE_BOXINESS_LRA_LU,
        zone_phase_issue: corr < ZONE_PHASE_ISSUE_CORRELATION,
        zone_harsh_resonance: peaks.iter().any(|&f| (2000.0..=8000.0).contains(&f)),
    }
}
