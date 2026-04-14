//! Stage 8 — TPDF Dither
//! Ported from sm-core. Critical adaptation: removed rand_core::RngCore dependency.
//! Uses inline SimpleRng trait — no external dependency permitted.
//! Authority: LineOS Constitution v2.0 §09.2 — no rand_core in production pipelines
//! dither_bits_24 and dither_bits_16 from PipelineConstants (bmr-128.schema.json).

use crate::types::audio::AudioChunk;

/// Inline RNG trait — replaces rand_core::RngCore.
/// No external dependency. Deterministic. Seedable.
pub trait SimpleRng {
    fn next_u32(&mut self) -> u32;
}

pub struct Stage8Dither<R: SimpleRng> {
    amplitude: f32,
    rng:       R,
}

impl<R: SimpleRng> Stage8Dither<R> {
    /// export_16bit: true → use dither_bits_16, false → dither_bits_24
    /// Both values from bmr-128.schema.json — never hardcoded.
    pub fn new(export_16bit: bool, rng: R, dither_bits_24: f32, dither_bits_16: f32) -> Self {
        let amplitude = if export_16bit { dither_bits_16 } else { dither_bits_24 };
        Self { amplitude, rng }
    }

    /// TPDF: Two independent uniform values → triangular distribution [-1, 1]
    fn tpdf_sample(&mut self) -> f32 {
        let r1 = (self.rng.next_u32() as f32) / (u32::MAX as f32);
        let r2 = (self.rng.next_u32() as f32) / (u32::MAX as f32);
        r1 - r2
    }

    pub fn process_chunk(&mut self, chunk: &mut AudioChunk) {
        for sample in chunk.samples.iter_mut() {
            let rnd = self.tpdf_sample();
            *sample += rnd * self.amplitude;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ZeroRng;
    impl SimpleRng for ZeroRng {
        fn next_u32(&mut self) -> u32 { 0 }
    }

    #[test]
    fn test_dither_zero_rng() {
        // ZeroRng: (0/MAX) - (0/MAX) = 0 → no modification
        let mut process = Stage8Dither::new(true, ZeroRng, 0.00000011920928955078125, 0.000030517578125);
        let mut chunk = AudioChunk {
            samples:     alloc::vec![0.0; 1024],
            sample_rate: 48000,
            channels:    2,
        };
        process.process_chunk(&mut chunk);
        for &s in &chunk.samples {
            assert_eq!(s, 0.0);
        }
    }
}
