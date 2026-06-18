//! 64-band log-spaced spectrum analyzer.
//! Zero heap allocation per compute() call.
//!
//! FFT size: 1024 points at 48 kHz → bin resolution = 46.875 Hz/bin.
//! Bands below ~47 Hz (bands 0–8) share FFT bin 1; this is expected
//! behaviour at this FFT size and produces a flat-ish bass shelf.

use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex;
use std::sync::Arc;

const FFT_SIZE: usize = 1024;
const N_BANDS: usize = 64;

pub struct SpectrumAnalyzer {
    fft: Arc<dyn RealToComplex<f32>>,
    scratch: Vec<f32>,
    output: Vec<Complex<f32>>,
    window: Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let scratch = fft.make_input_vec();
        let output = fft.make_output_vec();
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos())
            .collect();
        Self { fft, scratch, output, window }
    }

    /// Compute 64 log-spaced dBFS bands from interleaved PCM.
    ///
    /// `data`     – interleaved f32 samples (L,R,L,R... for stereo)
    /// `channels` – number of interleaved channels (1 = mono, 2 = stereo)
    ///
    /// Returns [-120.0; 64] on empty input or FFT failure.
    /// dBFS reference: 0 dBFS = full-scale amplitude normalised by FFT_SIZE.
    pub fn compute(&mut self, data: &[f32], channels: usize) -> [f32; 64] {
        let ch = channels.max(1);
        let frames = data.len() / ch;
        let n = frames.min(FFT_SIZE);
        if n == 0 {
            return [-120.0f32; 64];
        }

        // Deinterleave channel 0 into FFT scratch buffer with Hanning window.
        for i in 0..n {
            self.scratch[i] = data.get(i * ch).copied().unwrap_or(0.0) * self.window[i];
        }
        for i in n..FFT_SIZE {
            self.scratch[i] = 0.0;
        }

        if self.fft.process(&mut self.scratch, &mut self.output).is_err() {
            return [-120.0f32; 64];
        }

        // Map FFT bins → 64 log-spaced output bands (20 Hz – 20 kHz).
        // bin_hz = 46.875 Hz per bin at 48 kHz / FFT_SIZE=1024.
        let bin_hz = 48000.0f32 / FFT_SIZE as f32;
        let f_min = 20.0f32;
        let f_max = 20_000.0f32;
        let log_range = (f_max / f_min).ln();
        let mut out = [-120.0f32; N_BANDS];

        for (b, out_val) in out.iter_mut().enumerate() {
            let f_low  = f_min * (b as f32       / N_BANDS as f32 * log_range).exp();
            let f_high = f_min * ((b + 1) as f32 / N_BANDS as f32 * log_range).exp();

            // Skip DC (bin 0). Sub-resolution bands (<46.875 Hz) fold onto bin 1
            // so they share bass energy rather than staying at the -120 floor.
            let bin_low  = ((f_low  / bin_hz) as usize).max(1);
            let bin_high = ((f_high / bin_hz) as usize + 1)
                .max(bin_low + 1)      // always at least one bin wide
                .min(self.output.len());

            // Average power (|c|²) across the mapped FFT bins.
            let avg_power: f32 = self.output[bin_low..bin_high]
                .iter()
                .map(|c| c.norm_sqr())
                .sum::<f32>()
                / (bin_high - bin_low) as f32;

            // Amplitude-normalised dBFS:
            //   amp = sqrt(avg_power) / FFT_SIZE
            //   dB  = 20 * log10(amp)
            // Floor: -120 dBFS for signals below 1e-10 amplitude.
            let amp = avg_power.sqrt() / FFT_SIZE as f32;
            *out_val = if amp > 1e-10 { 20.0 * amp.log10() } else { -120.0 };
        }

        out
    }
}

impl Default for SpectrumAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
