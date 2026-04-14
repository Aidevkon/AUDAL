//! Stage 2 — EQ: HPF + Air Shelf
//! Ported from sm-core. Adapted for no_std + alloc.
//! Constants loaded from PipelineConstants (bmr-128.schema.json) — never hardcoded.

use alloc::vec::Vec;
use crate::dsp::biquad::Biquad;
use crate::types::audio::AudioChunk;

pub struct Stage2Eq {
    hpf_filters:        Vec<Biquad>,
    air_shelf_filters:  Vec<Biquad>,
    channels:           u16,
}

impl Stage2Eq {
    /// Create Stage2Eq with constants from PipelineConstants.
    /// eq_hpf_freq_hz, eq_air_shelf_hz from bmr-128.schema.json.
    pub fn new(
        sample_rate:      u32,
        channels:         u16,
        eq_hpf_freq_hz:   f32,
        eq_air_shelf_hz:  f32,
    ) -> Self {
        let mut hpf_filters       = Vec::with_capacity(channels as usize);
        let mut air_shelf_filters = Vec::with_capacity(channels as usize);

        for _ in 0..channels {
            let mut hpf = Biquad::new();
            hpf.set_hpf(eq_hpf_freq_hz, sample_rate as f32, 0.707);
            hpf_filters.push(hpf);

            let mut shelf = Biquad::new();
            shelf.set_high_shelf(eq_air_shelf_hz, sample_rate as f32, 1.5, 0.707);
            air_shelf_filters.push(shelf);
        }

        Self { hpf_filters, air_shelf_filters, channels }
    }

    pub fn process_chunk(&mut self, chunk: &mut AudioChunk) {
        let frames = chunk.frame_count();
        let chans  = self.channels as usize;

        for frame in 0..frames {
            for ch in 0..chans {
                let idx = frame * chans + ch;
                let s = chunk.samples[idx];
                let s = self.hpf_filters[ch].process(s);
                let s = self.air_shelf_filters[ch].process(s);
                chunk.samples[idx] = s;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eq_silence() {
        // 30.0 and 12000.0 from bmr-128.schema.json pipeline section
        let mut eq = Stage2Eq::new(48000, 2, 30.0, 12000.0);
        let mut chunk = AudioChunk {
            samples:     alloc::vec![0.0; 1024],
            sample_rate: 48000,
            channels:    2,
        };
        eq.process_chunk(&mut chunk);
        for &s in &chunk.samples {
            assert_eq!(s, 0.0);
        }
    }
}
