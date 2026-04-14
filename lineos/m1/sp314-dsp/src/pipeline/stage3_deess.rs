//! Stage 3 — Sidechain De-esser
//! Ported from sm-core. Adapted for no_std + alloc.
//! Threshold: hardcoded -18dB sidechain, ratio 4:1 (acceptable — not a BMR-128 platform target)

use alloc::vec::Vec;
use crate::dsp::biquad::Biquad;
use crate::types::audio::AudioChunk;

pub struct Stage3DeEss {
    bpf_filters:      Vec<Biquad>,
    envelope_states:  Vec<f32>,
    channels:         u16,
    threshold:        f32,
    ratio:            f32,
    attack_coef:      f32,
    release_coef:     f32,
}

impl Stage3DeEss {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let mut bpf_filters     = Vec::with_capacity(channels as usize);
        let mut envelope_states = Vec::with_capacity(channels as usize);

        for _ in 0..channels {
            let mut bpf = Biquad::new();
            bpf.set_hpf(6000.0, sample_rate as f32, 0.707);
            bpf_filters.push(bpf);
            envelope_states.push(0.0);
        }

        // Attack: 1ms, Release: 50ms
        let attack_coef  = libm::expf(-1.0 / (0.001 * sample_rate as f32));
        let release_coef = libm::expf(-1.0 / (0.050 * sample_rate as f32));

        Self {
            bpf_filters,
            envelope_states,
            channels,
            threshold:  -18.0,  // De-esser internal sidechain threshold (dBFS) — not a platform target
            ratio:      4.0,
            attack_coef,
            release_coef,
        }
    }

    pub fn process_chunk(&mut self, chunk: &mut AudioChunk) {
        let frames = chunk.frame_count();
        let chans  = self.channels as usize;

        for frame in 0..frames {
            for ch in 0..chans {
                let idx    = frame * chans + ch;
                let sample = chunk.samples[idx];

                // 1. Sidechain filter
                let sc_sample = self.bpf_filters[ch].process(sample);

                // 2. Envelope follower
                let sc_abs = libm::fabsf(sc_sample);
                let env = if sc_abs > self.envelope_states[ch] {
                    self.attack_coef * self.envelope_states[ch] + (1.0 - self.attack_coef) * sc_abs
                } else {
                    self.release_coef * self.envelope_states[ch] + (1.0 - self.release_coef) * sc_abs
                };
                self.envelope_states[ch] = env;

                // 3. Gain computer
                let mut gain = 1.0f32;
                let env_db = 20.0 * libm::log10f(env + 1e-9);

                if env_db > self.threshold {
                    let over      = env_db - self.threshold;
                    let compressed = over / self.ratio;
                    let makeup    = over - compressed;
                    gain = libm::powf(10.0, -makeup / 20.0);
                }

                // 4. VCA
                chunk.samples[idx] = sample * gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deess_silence() {
        let mut process = Stage3DeEss::new(48000, 2);
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
