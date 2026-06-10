//! 64-band log-spaced spectrum analyzer.
//! Zero heap allocation per compute() call.

use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex;
use std::sync::Arc;

const FFT_SIZE: usize = 1024;
const N_BANDS:  usize = 64;

pub struct SpectrumAnalyzer {
    fft:     Arc<dyn RealToComplex<f32>>,
    scratch: Vec<f32>,
    output:  Vec<Complex<f32>>,
    window:  Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft     = planner.plan_fft_forward(FFT_SIZE);
        let scratch = fft.make_input_vec();
        let output  = fft.make_output_vec();
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI
                * i as f32 / FFT_SIZE as f32).cos())
            .collect();
        Self { fft, scratch, output, window }
    }

    /// Zero-allocation spectrum compute from interleaved audio.
    /// Reads channel 0 directly into pre-allocated scratch buffer.
    pub fn compute(&mut self, data: &[f32], channels: usize) -> [f32; 64] {
        let ch     = channels.max(1);
        let frames = data.len() / ch;
        let n      = frames.min(FFT_SIZE);
        if n == 0 { return [-120.0f32; 64]; }

        for i in 0..n {
            self.scratch[i] = data.get(i * ch).copied().unwrap_or(0.0)
                * self.window[i];
        }
        for i in n..FFT_SIZE { self.scratch[i] = 0.0; }

        if self.fft.process(&mut self.scratch, &mut self.output).is_err() {
            return [-120.0f32; 64];
        }

        let bin_hz    = 48000.0f32 / FFT_SIZE as f32;
        let f_min     = 20.0f32;
        let f_max     = 20000.0f32;
        let log_range = (f_max / f_min).ln();
        let mut out   = [-120.0f32; N_BANDS];

        for (b, out_val) in out.iter_mut().enumerate() {
            let f_low  = f_min * (b as f32 / N_BANDS as f32 * log_range).exp();
            let f_high = f_min * ((b + 1) as f32 / N_BANDS as f32 * log_range).exp();
            let bin_low  = ((f_low  / bin_hz) as usize).max(1);
            let bin_high = ((f_high / bin_hz) as usize + 1).min(self.output.len());
            if bin_low >= bin_high { continue; }
            let energy: f32 = self.output[bin_low..bin_high]
                .iter().map(|c| c.norm_sqr()).sum::<f32>()
                / (bin_high - bin_low) as f32;
            *out_val = if energy > 1e-15 {
                10.0 * energy.log10() - 20.0 * (FFT_SIZE as f32).log10()
            } else { -120.0 };
        }
        out
    }
}

impl Default for SpectrumAnalyzer {
    fn default() -> Self { Self::new() }
}
