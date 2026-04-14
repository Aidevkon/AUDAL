//! Stage 7 — True Peak Limiter with Lookahead
//! Ported from sm-core. Adapted for no_std + alloc.
//! lookahead_max and true_peak_ceiling from PipelineConstants (bmr-128.schema.json).
//! Authority: LineOS Constitution v2.0 §09.1 — libm only

use crate::types::audio::AudioChunk;

pub struct Stage7Limit {
    channels:          u16,
    buffer:            alloc::vec::Vec<[f32; 2]>,
    write_idx:         usize,
    read_idx:          usize,
    active_len:        usize,
    ceiling_linear:    f32,
    release_coef_fast: f32,
    release_coef_slow: f32,
    gain_state:        f32,
}

impl Stage7Limit {
    /// lookahead_max and true_peak_ceiling_dbfs from bmr-128.schema.json
    pub fn new(
        sample_rate:            u32,
        channels:               u16,
        lookahead_max:          usize,
        true_peak_ceiling_dbfs: f32,
    ) -> Self {
        let active_len = ((sample_rate as f32 * 0.002) as usize).min(lookahead_max);

        let release_coef_fast = libm::expf(-1.0 / (0.020 * sample_rate as f32));
        let release_coef_slow = libm::expf(-1.0 / (0.100 * sample_rate as f32));
        let ceiling_linear    = libm::powf(10.0, true_peak_ceiling_dbfs / 20.0);

        let mut buffer = alloc::vec::Vec::with_capacity(lookahead_max);
        for _ in 0..lookahead_max {
            buffer.push([0.0f32; 2]);
        }

        Self {
            channels,
            buffer,
            write_idx: active_len.saturating_sub(1),
            read_idx: 0,
            active_len,
            ceiling_linear,
            release_coef_fast,
            release_coef_slow,
            gain_state: 1.0,
        }
    }

    pub fn process_chunk(&mut self, chunk: &mut AudioChunk) {
        let frames = chunk.frame_count();
        let chans  = self.channels as usize;

        for frame in 0..frames {
            let l_sample = if chans >= 1 { chunk.samples[frame * chans] }     else { 0.0 };
            let r_sample = if chans >= 2 { chunk.samples[frame * chans + 1] } else { 0.0 };

            self.buffer[self.write_idx] = [l_sample, r_sample];
            self.write_idx = (self.write_idx + 1) % self.active_len;
            self.read_idx  = (self.read_idx  + 1) % self.active_len;

            let out_l = self.buffer[self.read_idx][0];
            let out_r = self.buffer[self.read_idx][1];

            // Scan lookahead for true peak
            let mut tp_lookahead = 0.0f32;
            for i in 0..self.active_len {
                let sl = libm::fabsf(self.buffer[i][0]);
                let sr = libm::fabsf(self.buffer[i][1]);
                if sl > tp_lookahead { tp_lookahead = sl; }
                if sr > tp_lookahead { tp_lookahead = sr; }
            }

            // True peak overhead heuristic (+15%)
            tp_lookahead *= 1.15;

            let target_gain = if tp_lookahead > self.ceiling_linear {
                self.ceiling_linear / tp_lookahead
            } else {
                1.0
            };

            // Adaptive release
            if target_gain < self.gain_state {
                self.gain_state = target_gain; // instant attack
            } else {
                let rc = if self.gain_state < 0.5 {
                    self.release_coef_fast
                } else {
                    self.release_coef_slow
                };
                self.gain_state = rc * self.gain_state + (1.0 - rc) * target_gain;
            }

            if chans >= 1 { chunk.samples[frame * chans]     = out_l * self.gain_state; }
            if chans >= 2 { chunk.samples[frame * chans + 1] = out_r * self.gain_state; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limit_silence() {
        // 192 and -1.0 from bmr-128.schema.json
        let mut process = Stage7Limit::new(48000, 2, 192, -1.0);
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
