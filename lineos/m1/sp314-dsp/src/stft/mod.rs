use std::sync::Arc;
use rustfft::{FftPlanner, num_complex::Complex};

pub const FFT_SIZE: usize = 2048;
pub const HOP_SIZE: usize = 512;
pub const N_BINS:   usize = FFT_SIZE / 2 + 1;

pub struct StftEngine {
    fft_forward: Arc<dyn rustfft::Fft<f32>>,
    fft_inverse: Arc<dyn rustfft::Fft<f32>>,
    window:      Vec<f32>,
    scratch:     Vec<Complex<f32>>,
}

impl StftEngine {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft_forward = planner.plan_fft_forward(FFT_SIZE);
        let fft_inverse = planner.plan_fft_inverse(FFT_SIZE);
        let scratch_len = fft_forward
            .get_inplace_scratch_len()
            .max(fft_inverse.get_inplace_scratch_len());

        // Periodic Hann window:
        // w[n] = 0.5 - 0.5 * cos(2π * n / FFT_SIZE)
        // CRITICAL: periodic (divide by N), NOT symmetric (N-1)
        // Use libm::cosf — no std::f32 trig (constitutional rule)
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|n| {
                let theta = 2.0_f32
                    * core::f32::consts::PI
                    * n as f32
                    / FFT_SIZE as f32;
                0.5_f32 - 0.5_f32 * libm::cosf(theta)
            })
            .collect();

        Self {
            fft_forward,
            fft_inverse,
            window,
            scratch: vec![Complex::new(0.0, 0.0); scratch_len],
        }
    }

    /// Compute STFT of a mono signal.
    /// Returns Vec of complex spectra [n_frames][N_BINS].
    pub fn forward(&mut self, signal: &[f32])
        -> (Vec<Vec<Complex<f32>>>, usize)
    {
        let n = signal.len();
        let mut frames = Vec::new();
        let mut buf = vec![Complex::new(0.0_f32, 0.0_f32);
                           FFT_SIZE];
        let mut pos = 0;

        while pos + FFT_SIZE <= n {
            for j in 0..FFT_SIZE {
                buf[j] = Complex::new(
                    signal[pos + j] * self.window[j], 0.0_f32);
            }
            self.fft_forward
                .process_with_scratch(&mut buf, &mut self.scratch);
            let mut frame = vec![Complex::new(0.0_f32, 0.0_f32);
                                 N_BINS];
            frame.copy_from_slice(&buf[..N_BINS]);
            frames.push(frame);
            pos += HOP_SIZE;
        }
        let n_frames = frames.len();
        (frames, n_frames)
    }

    /// Compute iSTFT using Weighted Overlap-Add (WOLA).
    /// OLA sum for periodic Hann at 75% overlap = 1.5 (proven)
    pub fn inverse(&mut self,
                   frames: &[Vec<Complex<f32>>],
                   output_len: usize) -> Vec<f32>
    {
        let mut output     = vec![0.0_f32; output_len];
        let mut window_sum = vec![0.0_f32; output_len];
        let mut buf = vec![Complex::new(0.0_f32, 0.0_f32);
                           FFT_SIZE];

        for (k, spectrum) in frames.iter().enumerate() {
            buf[..N_BINS].copy_from_slice(spectrum);
            for i in 1..N_BINS - 1 {
                buf[FFT_SIZE - i] = spectrum[i].conj();
            }
            self.fft_inverse
                .process_with_scratch(&mut buf, &mut self.scratch);

            let start = k * HOP_SIZE;
            let end   = (start + FFT_SIZE).min(output_len);
            let len   = end - start;

            for i in 0..len {
                let sample = buf[i].re / FFT_SIZE as f32;
                output[start + i]     += sample * self.window[i];
                window_sum[start + i] += self.window[i]
                                         * self.window[i];
            }
        }

        for i in 0..output_len {
            if window_sum[i] > 1e-8_f32 {
                output[i] /= window_sum[i];
            }
        }
        output
    }
}

pub mod spectral_flux;
pub use spectral_flux::SpectralFluxDetector;
