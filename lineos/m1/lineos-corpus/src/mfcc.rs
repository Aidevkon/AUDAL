// allow: index loops here are triangular filterbank ranges over
// computed bin boundaries, DCT basis math, and a zero-allocation
// lockstep window multiply; iterator forms would obscure the math.
#![allow(clippy::needless_range_loop)]

//! MFCC — Mel-Frequency Cepstral Coefficients
//! Authority: corpus-learning-spec-v1_2.md
//! Zero heap allocation in compute() — all buffers pre-allocated.

use realfft::RealFftPlanner;
use rustfft::num_complex::Complex;

pub const N_MFCC: usize = 13;
pub const N_FILTERS: usize = 26;
pub const FFT_SIZE: usize = 1024;
pub const SAMPLE_RATE: f32 = 48000.0;
pub const F_MIN: f32 = 20.0;
pub const F_MAX: f32 = 8000.0;

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * libm::log10f(1.0 + hz / 700.0)
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (libm::powf(10.0, mel / 2595.0) - 1.0)
}

pub struct MelFilterbank {
    pub weights: Vec<Vec<f32>>,
    pub n_bins: usize,
}

impl Default for MelFilterbank {
    fn default() -> Self {
        Self::new()
    }
}

impl MelFilterbank {
    pub fn new() -> Self {
        let n_bins = FFT_SIZE / 2 + 1;
        let mel_min = hz_to_mel(F_MIN);
        let mel_max = hz_to_mel(F_MAX);

        let mel_points: Vec<f32> = (0..=(N_FILTERS + 1))
            .map(|i| mel_min + i as f32 * (mel_max - mel_min) / (N_FILTERS + 1) as f32)
            .collect();

        let bin_points: Vec<usize> = mel_points
            .iter()
            .map(|&m| {
                let hz = mel_to_hz(m);
                let bin = (hz / SAMPLE_RATE * FFT_SIZE as f32) as usize;
                bin.min(n_bins - 1)
            })
            .collect();

        let mut weights = vec![vec![0.0f32; n_bins]; N_FILTERS];
        for f in 0..N_FILTERS {
            let left = bin_points[f];
            let center = bin_points[f + 1];
            let right = bin_points[f + 2];
            for b in left..center {
                if center > left {
                    weights[f][b] = (b - left) as f32 / (center - left) as f32;
                }
            }
            for b in center..right {
                if right > center {
                    weights[f][b] = (right - b) as f32 / (right - center) as f32;
                }
            }
            if center < n_bins {
                weights[f][center] = 1.0;
            }
        }
        Self { weights, n_bins }
    }

    pub fn apply(&self, power_spectrum: &[f32]) -> [f32; N_FILTERS] {
        let mut energies = [0.0f32; N_FILTERS];
        for f in 0..N_FILTERS {
            let energy: f32 = self.weights[f]
                .iter()
                .zip(power_spectrum.iter())
                .map(|(&w, &p)| w * p)
                .sum();
            // Epsilon clamp prevents log10(0) = -inf → NaN cascade
            energies[f] = libm::log10f(energy.max(1e-10));
        }
        energies
    }
}

fn dct(input: &[f32; N_FILTERS]) -> [f32; N_MFCC] {
    let n = N_FILTERS as f32;
    let mut output = [0.0f32; N_MFCC];
    for k in 0..N_MFCC {
        let mut sum = 0.0f32;
        for (i, &x) in input.iter().enumerate() {
            sum += x * libm::cosf(
                core::f32::consts::PI * k as f32 * (2.0 * i as f32 + 1.0) / (2.0 * n),
            );
        }
        output[k] = sum
            * if k == 0 {
                libm::sqrtf(1.0 / n)
            } else {
                libm::sqrtf(2.0 / n)
            };
    }
    output
}

/// MFCC analyzer — zero allocation in compute() hot path.
pub struct MfccAnalyzer {
    fft: std::sync::Arc<dyn realfft::RealToComplex<f32>>,
    scratch: Vec<f32>,
    output: Vec<Complex<f32>>,
    power: Vec<f32>, // pre-allocated power spectrum buffer
    window: Vec<f32>,
    filterbank: MelFilterbank,
}

impl MfccAnalyzer {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let scratch = fft.make_input_vec();
        let output = fft.make_output_vec();
        let n_bins = FFT_SIZE / 2 + 1;
        let power = vec![0.0f32; n_bins]; // allocated ONCE here
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 - 0.5 * libm::cosf(2.0 * core::f32::consts::PI * i as f32 / FFT_SIZE as f32)
            })
            .collect();
        Self {
            fft,
            scratch,
            output,
            power,
            window,
            filterbank: MelFilterbank::new(),
        }
    }

    /// Compute 13 MFCC coefficients from a mono audio window.
    /// ZERO heap allocation — all buffers are pre-allocated in new().
    pub fn compute(&mut self, signal: &[f32]) -> [f32; N_MFCC] {
        let n = signal.len().min(FFT_SIZE);
        if n == 0 {
            return [0.0f32; N_MFCC];
        }

        for i in 0..n {
            self.scratch[i] = signal[i] * self.window[i];
        }
        for i in n..FFT_SIZE {
            self.scratch[i] = 0.0;
        }

        if self
            .fft
            .process(&mut self.scratch, &mut self.output)
            .is_err()
        {
            return [0.0f32; N_MFCC];
        }

        // Power spectrum — write into pre-allocated buffer (ZERO allocation)
        let n_bins = FFT_SIZE / 2 + 1;
        for (i, c) in self.output.iter().take(n_bins).enumerate() {
            self.power[i] = c.norm_sqr() / FFT_SIZE as f32;
        }

        let log_energies = self.filterbank.apply(&self.power);
        dct(&log_energies)
    }

    /// Compute 13 MFCC coefficients over a signal by averaging non-overlapping
    /// FFT_SIZE frames.
    pub fn compute_windowed(&mut self, signal: &[f32]) -> [f32; N_MFCC] {
        if signal.len() <= FFT_SIZE {
            return self.compute(signal);
        }

        let mut sum = [0.0f32; N_MFCC];
        let mut count = 0;
        let hop = FFT_SIZE; // non-overlapping

        let mut start = 0;
        // Note: this drops the tail of the signal. For a 5s window, 234 frames
        // cover 239,616 of 240,000 samples (dropping ~0.16%, which is fine).
        // For shorter windows (e.g. 100ms corpus windows) this drops ~15%.
        while start + FFT_SIZE <= signal.len() {
            let chunk = &signal[start..start + FFT_SIZE];
            let mfcc = self.compute(chunk);
            for i in 0..N_MFCC {
                sum[i] += mfcc[i];
            }
            count += 1;
            start += hop;
        }

        if count == 0 {
            return self.compute(signal);
        }

        let inv = 1.0 / count as f32;
        for i in 0..N_MFCC {
            sum[i] *= inv;
        }
        sum
    }
}

impl Default for MfccAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mfcc_returns_13_coefficients() {
        let mut a = MfccAnalyzer::new();
        let signal: Vec<f32> = (0..1024)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0))
            .collect();
        let mfcc = a.compute(&signal);
        assert_eq!(mfcc.len(), 13);
    }

    #[test]
    fn mfcc_silence_is_finite() {
        let mut a = MfccAnalyzer::new();
        let mfcc = a.compute(&vec![0.0f32; 1024]);
        for &c in &mfcc {
            assert!(c.is_finite(), "coefficient must be finite");
        }
    }

    #[test]
    fn mfcc_different_frequencies_differ() {
        let mut a = MfccAnalyzer::new();
        let s440: Vec<f32> = (0..1024)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0))
            .collect();
        let s4000: Vec<f32> = (0..1024)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 4000.0 * i as f32 / 48000.0))
            .collect();
        let diff: f32 = a
            .compute(&s440)
            .iter()
            .zip(a.compute(&s4000).iter())
            .map(|(x, y)| (x - y).abs())
            .sum();
        assert!(
            diff > 0.1,
            "Different frequencies must yield different MFCCs"
        );
    }

    #[test]
    fn mfcc_deterministic() {
        let signal: Vec<f32> = (0..1024)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 1000.0 * i as f32 / 48000.0))
            .collect();
        assert_eq!(
            MfccAnalyzer::new().compute(&signal),
            MfccAnalyzer::new().compute(&signal),
            "MFCCs must be deterministic"
        );
    }

    #[test]
    fn mel_filterbank_all_filters_nonzero() {
        let fb = MelFilterbank::new();
        for (f, filter) in fb.weights.iter().enumerate() {
            let sum: f32 = filter.iter().sum();
            assert!(sum > 0.0, "Filter {} has zero weight sum", f);
        }
    }
}
