// allow: index loops here iterate 2-D complex STFT frames
// (frames[t][b]) with per-bin magnitude math; iterator rewrites
// obscure the time×frequency structure.
#![allow(clippy::needless_range_loop)]

// analysis/spectral.rs — Spectral feature extraction
// libm only. No std::f32 methods.
// Per-channel computation → averaged per S-002 §4 (Option A)

use crate::stft::StftEngine;
use crate::stft::N_BINS;
use crate::stft::StreamingStftEncoder;

use rustfft::{num_complex::Complex, FftPlanner};

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

/// Measure total energy in a frequency band.
/// Returns sqrt of sum of squared magnitudes
/// across bins in [low_hz, high_hz].
///
/// Applies Hann window to reduce spectral
/// leakage. Normalizes by N to correct for
/// rustfft's unscaled output.
/// Returns total band energy (not per-bin avg).
pub fn measure_band_energy_hz(signal: &[f32], sample_rate: u32, low_hz: f32, high_hz: f32) -> f32 {
    let n = signal.len();
    if n == 0 {
        return 0.0;
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);

    // Hann window — reduces spectral leakage
    let mut buffer: Vec<Complex<f32>> = signal
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos());
            Complex { re: s * w, im: 0.0 }
        })
        .collect();

    fft.process(&mut buffer);

    // Normalize by N (rustfft is unscaled)
    let n_f = n as f32;
    let bin_hz = sample_rate as f32 / n_f;
    let low_bin = (low_hz / bin_hz).floor() as usize;
    let high_bin = (high_hz / bin_hz).ceil() as usize;
    let high_bin = high_bin.min(n / 2);

    if low_bin >= high_bin {
        return 0.0;
    }

    // Total band energy, normalized by N²
    let energy: f32 = buffer[low_bin..high_bin]
        .iter()
        .map(|c| c.norm_sqr() / (n_f * n_f))
        .sum();

    energy.sqrt()
}

#[cfg(test)]
mod band_energy_tests {
    use super::*;

    #[test]
    fn band_energy_sine_in_band() {
        // 250Hz sine should have high energy
        // in 200-300Hz band
        let sr = 48000u32;
        let n = sr as usize;
        let signal: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 250.0 * i as f32 / sr as f32).sin())
            .collect();
        let in_band = measure_band_energy_hz(&signal, sr, 200.0, 300.0);
        let out_band = measure_band_energy_hz(&signal, sr, 500.0, 2000.0);
        assert!(
            in_band > out_band * 10.0,
            "250Hz sine energy should be \
             dominant in 200-300Hz band: \
             in={:.6} out={:.6}",
            in_band,
            out_band
        );
    }
}

pub struct StreamingSpectralAnalyzer {
    encoder: StreamingStftEncoder,
    sample_rate: u32,
    centroid_total: f32,
    centroid_valid_frames: usize,
    flatness_total: f32,
    flatness_valid_frames: usize,
    crest_total: f32,
    crest_valid_frames: usize,
    was_empty: bool,
}

impl StreamingSpectralAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            encoder: StreamingStftEncoder::new(),
            sample_rate,
            centroid_total: 0.0,
            centroid_valid_frames: 0,
            flatness_total: 0.0,
            flatness_valid_frames: 0,
            crest_total: 0.0,
            crest_valid_frames: 0,
            was_empty: true,
        }
    }

    pub fn feed_chunk(&mut self, chunk: &[f32]) {
        if !chunk.is_empty() {
            self.was_empty = false;
        }
        let frames = self.encoder.feed_chunk(chunk);
        for frame in frames {
            self.process_frame(&frame);
        }
    }

    fn process_frame(&mut self, frame: &[Complex<f32>]) {
        use super::features::ANALYSIS_FFT_SIZE;
        let sample_rate = self.sample_rate;
        let bin_hz = sample_rate as f32 / (ANALYSIS_FFT_SIZE as f32 * 2.0);
        let eps = 1e-10_f32;
        let n_bins = N_BINS as f32;

        let mut c_weighted_sum = 0.0_f32;
        let mut c_mag_sum = 0.0_f32;
        let mut f_log_sum = 0.0_f32;
        let mut f_arith = 0.0_f32;
        let mut cr_max_mag = 0.0_f32;
        let mut cr_mag_sum = 0.0_f32;

        for b in 0..N_BINS {
            let re = frame[b].re;
            let im = frame[b].im;
            let mag = libm::sqrtf(re * re + im * im);
            
            let freq = b as f32 * bin_hz;
            c_weighted_sum += freq * mag;
            c_mag_sum += mag;
            
            f_log_sum += libm::logf(mag + eps);
            f_arith += mag;
            
            if mag > cr_max_mag {
                cr_max_mag = mag;
            }
            cr_mag_sum += mag;
        }

        if c_mag_sum > 1e-10 {
            self.centroid_total += c_weighted_sum / c_mag_sum;
            self.centroid_valid_frames += 1;
        }

        let f_geom = libm::expf(f_log_sum / n_bins);
        let f_amean = f_arith / n_bins;
        if f_amean > eps {
            self.flatness_total += f_geom / f_amean;
            self.flatness_valid_frames += 1;
        }

        let cr_mean = cr_mag_sum / n_bins;
        if cr_mean > 1e-10 {
            self.crest_total += 20.0 * libm::log10f(cr_max_mag / cr_mean);
            self.crest_valid_frames += 1;
        }
    }

    pub fn finish(mut self) -> (f32, f32, f32) {
        if self.was_empty {
            return (1000.0, 0.5, 10.0);
        }
        let encoder = std::mem::replace(&mut self.encoder, StreamingStftEncoder::new());
        let frames = encoder.finish();
        for frame in frames {
            self.process_frame(&frame);
        }
        
        let c = if self.centroid_valid_frames == 0 { 1000.0 } else { self.centroid_total / self.centroid_valid_frames as f32 };
        let f = if self.flatness_valid_frames == 0 { 0.5 } else { (self.flatness_total / self.flatness_valid_frames as f32).clamp(0.0, 1.0) };
        let cr = if self.crest_valid_frames == 0 { 10.0 } else { self.crest_total / self.crest_valid_frames as f32 };
        
        (c, f, cr)
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;

    #[test]
    fn streaming_spectral_matches_offline_reference() {
        let sr = 48000;
        let signal_lengths = [10240, 10000, 2048, 1023, 1, 0];
        let chunk_sizes = [4096, 1024, 512, 513, 1];

        let mut full_signal = vec![0.0_f32; 10240];
        let mut prng = 12345u32;
        for i in 0..full_signal.len() {
            let t = i as f32 / sr as f32;
            let sine = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * t);
            prng = prng.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (prng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            full_signal[i] = sine * 0.8 + noise * 0.2;
        }

        for &len in &signal_lengths {
            let signal = &full_signal[..len];

            let expected_c = spectral_centroid_hz(signal, sr);
            let expected_f = spectral_flatness(signal);
            let expected_cr = spectral_crest_factor_db(signal);

            for &cs in &chunk_sizes {
                let mut analyzer = StreamingSpectralAnalyzer::new(sr);
                for chunk in signal.chunks(cs) {
                    analyzer.feed_chunk(chunk);
                }
                let (c, f, cr) = analyzer.finish();
                assert_eq!(c, expected_c, "Centroid mismatch (len={}, chunk={})", len, cs);
                assert_eq!(f, expected_f, "Flatness mismatch (len={}, chunk={})", len, cs);
                assert_eq!(cr, expected_cr, "Crest mismatch (len={}, chunk={})", len, cs);
            }

            let mut analyzer = StreamingSpectralAnalyzer::new(sr);
            let mut pos = 0;
            let mut chunk_idx = 0;
            while pos < signal.len() {
                let cs = chunk_sizes[chunk_idx % chunk_sizes.len()];
                let end = (pos + cs).min(signal.len());
                analyzer.feed_chunk(&signal[pos..end]);
                pos = end;
                chunk_idx += 1;
            }
            let (c, f, cr) = analyzer.finish();
            assert_eq!(c, expected_c, "Centroid mismatch (len={}, chunk=mixed)", len);
            assert_eq!(f, expected_f, "Flatness mismatch (len={}, chunk=mixed)", len);
            assert_eq!(cr, expected_cr, "Crest mismatch (len={}, chunk=mixed)", len);
        }
    }
}
