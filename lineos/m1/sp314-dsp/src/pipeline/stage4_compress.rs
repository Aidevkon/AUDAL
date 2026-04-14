//! Stage 4 — RMS Compressor
//! Ported from sm-core. Adapted for no_std + alloc.
//! Thresholds loaded from PipelineConstants (bmr-128.schema.json) — never hardcoded.

use alloc::vec;
use alloc::vec::Vec;
use crate::types::audio::AudioChunk;

pub struct Stage4Compress {
    channels:      u16,
    threshold:     f32,
    ratio:         f32,
    knee:          f32,
    attack_coef:   f32,
    release_coef:  f32,
    env_states:    Vec<f32>,
}

impl Stage4Compress {
    /// Create Stage4Compress with constants from PipelineConstants.
    /// comp_threshold_dbfs, comp_ratio_default, comp_knee_db from bmr-128.schema.json.
    pub fn new(
        sample_rate:          u32,
        channels:             u16,
        comp_threshold_dbfs:  f32,
        comp_ratio_default:   f32,
        comp_knee_db:         f32,
    ) -> Self {
        let env_states = vec![0.0; channels as usize];

        // Default 10ms attack, 150ms release
        let attack_coef  = libm::expf(-1.0 / (0.010 * sample_rate as f32));
        let release_coef = libm::expf(-1.0 / (0.150 * sample_rate as f32));

        Self {
            channels,
            threshold: comp_threshold_dbfs,
            ratio:     comp_ratio_default,
            knee:      comp_knee_db,
            attack_coef,
            release_coef,
            env_states,
        }
    }

    pub fn process_chunk(&mut self, chunk: &mut AudioChunk) {
        let frames = chunk.frame_count();
        let chans  = self.channels as usize;

        // Auto makeup gain
        let makeup_db  = libm::fabsf(self.threshold * (1.0 - 1.0 / self.ratio));
        let makeup_lin = libm::powf(10.0, makeup_db / 20.0);

        for frame in 0..frames {
            // Stereo-linked max detection
            let mut max_env = 0.0f32;

            for ch in 0..chans {
                let idx        = frame * chans + ch;
                let sample_abs = libm::fabsf(chunk.samples[idx]);

                let env = if sample_abs > self.env_states[ch] {
                    self.attack_coef * self.env_states[ch] + (1.0 - self.attack_coef) * sample_abs
                } else {
                    self.release_coef * self.env_states[ch] + (1.0 - self.release_coef) * sample_abs
                };
                self.env_states[ch] = env;

                if env > max_env { max_env = env; }
            }

            // Gain computer (stereo linked)
            let mut gain = 1.0f32;
            let level_db = 20.0 * libm::log10f(max_env + 1e-9);

            if level_db > self.threshold - self.knee / 2.0 {
                let cv = if level_db > self.threshold + self.knee / 2.0 {
                    self.threshold + (level_db - self.threshold) / self.ratio
                } else {
                    // Soft knee polynomial
                    level_db
                        + (1.0 / self.ratio - 1.0)
                        * libm::powf(level_db - self.threshold + self.knee / 2.0, 2.0)
                        / (2.0 * self.knee)
                };
                let gain_db = cv - level_db;
                gain = libm::powf(10.0, gain_db / 20.0);
            }

            // VCA
            for ch in 0..chans {
                let idx = frame * chans + ch;
                chunk.samples[idx] = chunk.samples[idx] * gain * makeup_lin;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_silence() {
        // Values from bmr-128.schema.json
        let mut process = Stage4Compress::new(48000, 2, -18.0, 2.0, 6.0);
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
}
