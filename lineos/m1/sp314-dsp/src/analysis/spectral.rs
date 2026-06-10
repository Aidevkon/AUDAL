// analysis/spectral.rs — Spectral feature extraction
// libm only. No std::f32 methods.
// Per-channel computation → averaged per S-002 §4 (Option A)

use crate::stft::StftEngine;
use crate::stft::N_BINS;

/// Compute spectral centroid in Hz from a mono signal.
/// Averaged across all STFT frames.
pub fn spectral_centroid_hz(signal: &[f32], sample_rate: u32) -> f32 {
    if signal.is_empty() {
        return 1000.0;
    }

    let mut engine = StftEngine::new();
    let (frames, n_frames) = engine.forward(signal);

    if n_frames == 0 {
        return 1000.0;
    }

    let bin_hz = sample_rate as f32 / (ANALYSIS_FFT_SIZE as f32 * 2.0);
    let mut total_centroid = 0.0_f32;
    let mut valid_frames = 0usize;

    for t in 0..n_frames {
        let mut weighted_sum = 0.0_f32;
        let mut mag_sum = 0.0_f32;
        for b in 0..N_BINS {
            let re = frames[t][b].re;
            let im = frames[t][b].im;
            let mag = libm::sqrtf(re * re + im * im);
            let freq = b as f32 * bin_hz;
            weighted_sum += freq * mag;
            mag_sum += mag;
        }
        if mag_sum > 1e-10 {
            total_centroid += weighted_sum / mag_sum;
            valid_frames += 1;
        }
    }

    if valid_frames == 0 {
        1000.0
    } else {
        total_centroid / valid_frames as f32
    }
}

/// Spectral flatness: geometric / arithmetic mean of spectrum.
/// 0.0 = pure tone, 1.0 = white noise.
pub fn spectral_flatness(signal: &[f32]) -> f32 {
    if signal.is_empty() {
        return 0.5;
    }

    let mut engine = StftEngine::new();
    let (frames, n_frames) = engine.forward(signal);
    if n_frames == 0 {
        return 0.5;
    }

    let eps = 1e-10_f32;
    let mut total_flatness = 0.0_f32;
    let mut valid_frames = 0usize;

    for t in 0..n_frames {
        let mut log_sum = 0.0_f32;
        let mut arith = 0.0_f32;
        for b in 0..N_BINS {
            let re = frames[t][b].re;
            let im = frames[t][b].im;
            let mag = libm::sqrtf(re * re + im * im);
            log_sum += libm::logf(mag + eps);
            arith += mag;
        }
        let n = N_BINS as f32;
        let geom = libm::expf(log_sum / n);
        let amean = arith / n;
        if amean > eps {
            total_flatness += geom / amean;
            valid_frames += 1;
        }
    }

    if valid_frames == 0 {
        0.5
    } else {
        (total_flatness / valid_frames as f32).clamp(0.0, 1.0)
    }
}

/// Spectral crest factor in dB: max / mean magnitude per frame.
pub fn spectral_crest_factor_db(signal: &[f32]) -> f32 {
    if signal.is_empty() {
        return 10.0;
    }

    let mut engine = StftEngine::new();
    let (frames, n_frames) = engine.forward(signal);
    if n_frames == 0 {
        return 10.0;
    }

    let mut total = 0.0_f32;
    let mut valid = 0usize;

    for t in 0..n_frames {
        let mut max_mag = 0.0_f32;
        let mut mag_sum = 0.0_f32;
        for b in 0..N_BINS {
            let re = frames[t][b].re;
            let im = frames[t][b].im;
            let mag = libm::sqrtf(re * re + im * im);
            if mag > max_mag {
                max_mag = mag;
            }
            mag_sum += mag;
        }
        let mean = mag_sum / N_BINS as f32;
        if mean > 1e-10 {
            total += 20.0 * libm::log10f(max_mag / mean);
            valid += 1;
        }
    }

    if valid == 0 {
        10.0
    } else {
        total / valid as f32
    }
}

use super::features::ANALYSIS_FFT_SIZE;
