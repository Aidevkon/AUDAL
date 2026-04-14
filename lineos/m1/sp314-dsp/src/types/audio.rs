//! Audio chunk type — the fundamental unit of audio transport in sp314-dsp.
//! Ported from sm-core. Adapted for no_std + alloc.

use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// Interleaved PCM samples (f32, normalized to [-1.0, 1.0])
    pub samples:     Vec<f32>,
    pub sample_rate: u32,
    pub channels:    u16,
}

impl AudioChunk {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self { samples, sample_rate, channels }
    }

    /// Number of frames (samples per channel)
    #[inline]
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 { 0 } else { self.samples.len() / self.channels as usize }
    }
}
