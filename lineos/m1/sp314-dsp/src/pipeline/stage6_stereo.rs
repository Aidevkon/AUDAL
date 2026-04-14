//! Stage 6 — Mid/Side Stereo Processing
//! Ported from sm-core. Adapted for no_std + alloc.
//! ms_side_gain_db and ms_side_hpf_hz from PipelineConstants (bmr-128.schema.json).

use crate::dsp::biquad::Biquad;
use crate::types::audio::AudioChunk;

// 1/√2 constant — pure algebraic, no libm::sqrt at runtime
const FRAC_1_SQRT_2: f32 = 0.7071067811865476;

pub struct Stage6Stereo {
    channels:         u16,
    side_gain_linear: f32,
    hpf:              Biquad,
}

impl Stage6Stereo {
    /// ms_side_gain_db and ms_side_hpf_hz from bmr-128.schema.json
    pub fn new(sample_rate: u32, channels: u16, ms_side_gain_db: f32, ms_side_hpf_hz: f32) -> Self {
        let mut hpf = Biquad::new();
        hpf.set_hpf(ms_side_hpf_hz, sample_rate as f32, 0.707);

        Self {
            channels,
            side_gain_linear: libm::powf(10.0, ms_side_gain_db / 20.0),
            hpf,
        }
    }

    pub fn process_chunk(&mut self, chunk: &mut AudioChunk) {
        // Mono passthrough
        if self.channels < 2 {
            return;
        }
        if libm::fabsf(self.side_gain_linear - 1.0) < 1e-5 {
            return;
        }

        let frames = chunk.frame_count();
        let chans  = self.channels as usize;

        for frame in 0..frames {
            let idx_l = frame * chans;
            let idx_r = frame * chans + 1;

            let l = chunk.samples[idx_l];
            let r = chunk.samples[idx_r];

            // Encode M/S
            let mid  = (l + r) * FRAC_1_SQRT_2;
            let side = (l - r) * FRAC_1_SQRT_2;

            // HPF + width gain on side channel
            let side_filtered = self.hpf.process(side);
            let side_gained   = side_filtered * self.side_gain_linear;

            // Decode back to L/R
            chunk.samples[idx_l] = (mid + side_gained) * FRAC_1_SQRT_2;
            chunk.samples[idx_r] = (mid - side_gained) * FRAC_1_SQRT_2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stereo_silence() {
        // 1.5 and 120.0 from bmr-128.schema.json
        let mut process = Stage6Stereo::new(48000, 2, 1.5, 120.0);
        let mut chunk = AudioChunk {
            samples:     alloc::vec![0.0; 1024],
            sample_rate: 48000,
            channels:    2,
        };
        process.process_chunk(&mut chunk);
        for &s in &chunk.samples {
            assert!(s.abs() < 1e-6);
        }
    }

    #[test]
    fn test_mono_passthrough() {
        let mut process = Stage6Stereo::new(48000, 1, 1.5, 120.0);
        let mut chunk = AudioChunk {
            samples:     alloc::vec![0.5; 64],
            sample_rate: 48000,
            channels:    1,
        };
        process.process_chunk(&mut chunk);
        // Mono passthrough — untouched
        for &s in &chunk.samples {
            assert!((s - 0.5).abs() < 1e-6);
        }
    }
}
