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

impl Default for StftEngine {
    fn default() -> Self {
        Self::new()
    }
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
        let pad = FFT_SIZE / 2;
        let mut padded = vec![0.0f32; pad];
        padded.extend_from_slice(signal);
        padded.extend(vec![0.0f32; pad]);
        let signal = &padded;

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
        let pad = FFT_SIZE / 2;
        let internal_len = output_len + 2 * pad;
        let mut output     = vec![0.0_f32; internal_len];
        let mut window_sum = vec![0.0_f32; internal_len];
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
            let end   = (start + FFT_SIZE).min(internal_len);
            let len   = end - start;

            for i in 0..len {
                let sample = buf[i].re / FFT_SIZE as f32;
                output[start + i]     += sample * self.window[i];
                window_sum[start + i] += self.window[i]
                                         * self.window[i];
            }
        }

        for i in 0..internal_len {
            if window_sum[i] > 1e-8_f32 {
                output[i] /= window_sum[i];
            }
        }
        output[pad..pad + output_len].to_vec()
    }
}

/// Overlap-Add ring buffer for chunk-boundary seamless stitching.
/// Holds FFT_SIZE/2 = 1024 samples of overlap from previous chunk.
/// INV-ST-4: overlap size is always FFT_SIZE / 2.
pub struct OlaRingBuffer {
    /// Tail of previous iSTFT output — mixed into start of next chunk.
    overlap: Vec<f32>,
}

impl OlaRingBuffer {
    pub fn new() -> Self {
        Self {
            overlap: vec![0.0_f32; FFT_SIZE / 2],
        }
    }

    /// Mix overlap from previous chunk into the start of `output`.
    /// Saves tail of `output` as new overlap for next call.
    /// Returns the clean, overlap-corrected output.
    pub fn process(&mut self, output: &[f32]) -> Vec<f32> {
        let overlap_len = self.overlap.len();
        let out_len     = output.len();

        let mut result = output.to_vec();

        // Mix previous overlap into the start of this chunk
        let mix_len = overlap_len.min(out_len);
        for i in 0..mix_len {
            result[i] += self.overlap[i];
        }

        // Save tail of this chunk as new overlap
        if out_len >= overlap_len {
            self.overlap.copy_from_slice(&result[out_len - overlap_len..]);
            result.truncate(out_len - overlap_len);
        } else {
            // Chunk shorter than overlap — pad overlap
            self.overlap[..out_len].copy_from_slice(&result);
            self.overlap[out_len..].fill(0.0_f32);
            result.clear();
        }

        result
    }

    /// Flush remaining overlap at end of stream.
    /// Call once after all chunks are processed.
    pub fn flush(&self) -> Vec<f32> {
        self.overlap.clone()
    }
}

/// Streaming STFT context — processes one chunk at a time.
/// Maintains internal state for overlap between chunks.
/// INV-ST-5: chunk_size = 65536 samples (constitutional).
pub struct StftStreamContext {
    engine:    StftEngine,
    ola:       OlaRingBuffer,
    /// Lookahead buffer: holds last FFT_SIZE - HOP_SIZE samples
    /// from previous chunk for correct frame boundaries.
    lookahead: Vec<f32>,
}

impl StftStreamContext {
    pub fn new() -> Self {
        Self {
            engine:    StftEngine::new(),
            ola:       OlaRingBuffer::new(),
            lookahead: vec![0.0_f32; FFT_SIZE - HOP_SIZE],
        }
    }

    /// Forward STFT on a chunk. Returns magnitude frames.
    /// Prepends lookahead from previous chunk for correct boundaries.
    pub fn forward_chunk(&mut self, chunk: &[f32])
        -> Vec<Vec<f32>>
    {
        // Prepend lookahead so frames at chunk boundary are correct
        let mut buf = self.lookahead.clone();
        buf.extend_from_slice(chunk);

        // Update lookahead: last FFT_SIZE - HOP_SIZE samples
        let tail_start = buf.len().saturating_sub(FFT_SIZE - HOP_SIZE);
        self.lookahead = buf[tail_start..].to_vec();

        // STFT forward
        let (frames, n_frames) = self.engine.forward(&buf);

        // Extract magnitude
        frames.into_iter().take(n_frames).map(|frame| {
            frame.iter().take(N_BINS).map(|c| {
                libm::sqrtf(c.re * c.re + c.im * c.im)
            }).collect()
        }).collect()
    }

    /// Inverse STFT + OLA for a chunk of magnitude frames + phases.
    /// Returns overlap-corrected PCM samples.
    pub fn inverse_chunk(
        &mut self,
        frames: &[Vec<Complex<f32>>],
        original_len: usize,
    ) -> Vec<f32> {
        let raw = self.engine.inverse(frames, original_len);
        self.ola.process(&raw)
    }

    /// Flush remaining overlap at end of stream.
    pub fn flush(&self) -> Vec<f32> {
        self.ola.flush()
    }
}

pub mod spectral_flux;
pub use spectral_flux::SpectralFluxDetector;

pub mod hpss;
pub use hpss::HpssProcessor;

pub mod stem_renderer;
pub use stem_renderer::StemRenderer;

pub mod nmf;
pub use nmf::NmfEngine;
pub mod two_pass;
pub use two_pass::TwoPassEngine;

#[cfg(test)]
mod streaming_tests {
    use super::*;

    #[test]
    fn ola_ring_buffer_processes_without_panic() {
        let mut ola = OlaRingBuffer::new();
        let chunk = vec![0.1_f32; 65536];
        let result = ola.process(&chunk);
        assert!(!result.is_empty());
    }

    #[test]
    fn ola_ring_buffer_flush_returns_overlap_size() {
        let ola = OlaRingBuffer::new();
        let flushed = ola.flush();
        assert_eq!(flushed.len(), FFT_SIZE / 2);
    }

    #[test]
    fn stft_stream_context_forward_chunk_returns_frames() {
        let mut ctx = StftStreamContext::new();
        // 65536 samples = ~1.37s at 48kHz
        let chunk: Vec<f32> = (0..65536)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI
                * 440.0 * i as f32 / 48000.0))
            .collect();
        let frames = ctx.forward_chunk(&chunk);
        assert!(!frames.is_empty(), "Must produce frames");
        assert_eq!(frames[0].len(), N_BINS,
            "Each frame must have N_BINS magnitude bins");
    }

    #[test]
    fn stft_stream_context_deterministic() {
        // INV-AB-1: same chunk → same frames
        let chunk: Vec<f32> = (0..65536)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI
                * 1000.0 * i as f32 / 48000.0))
            .collect();

        let mut ctx1 = StftStreamContext::new();
        let frames1 = ctx1.forward_chunk(&chunk);

        let mut ctx2 = StftStreamContext::new();
        let frames2 = ctx2.forward_chunk(&chunk);

        assert_eq!(frames1.len(), frames2.len());
        for (f1, f2) in frames1.iter().zip(frames2.iter()) {
            for (a, b) in f1.iter().zip(f2.iter()) {
                assert!((a - b).abs() < 1e-6_f32,
                    "INV-AB-1 violation: frames differ");
            }
        }
    }
}
