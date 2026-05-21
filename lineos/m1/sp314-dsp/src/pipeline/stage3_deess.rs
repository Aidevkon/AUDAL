//! Stage 3 — Sidechain De-esser
//! Ported from sm-core. Adapted for no_std + alloc.
//! dess_band_low_hz / dess_band_high_hz from PipelineConstants (bmr-128.schema.json).
//! Threshold: hardcoded -18dB sidechain, ratio 4:1 (acceptable — not a BMR-128 platform target)

use alloc::vec::Vec;
use crate::dsp::biquad::Biquad;
use crate::types::audio::AudioChunk;

pub struct Stage3DeEss {
    sc_hpf_filters:   Vec<Biquad>,
    sc_lpf_filters:   Vec<Biquad>,
    envelope_states:  Vec<f32>,
    channels:         u16,
    threshold:        f32,
    ratio:            f32,
    attack_coef:      f32,
    release_coef:     f32,
}

impl Stage3DeEss {
    /// dess_band_low_hz and dess_band_high_hz from bmr-128.schema.json pipeline section.
    pub fn new(
        sample_rate:        u32,
        channels:           u16,
        dess_band_low_hz:   f32,
        dess_band_high_hz:  f32,
    ) -> Self {
        let mut sc_hpf_filters  = Vec::with_capacity(channels as usize);
        let mut sc_lpf_filters  = Vec::with_capacity(channels as usize);
        let mut envelope_states = Vec::with_capacity(channels as usize);
        let sr = sample_rate as f32;

        for _ in 0..channels {
            let mut hpf = Biquad::new();
            hpf.set_hpf(dess_band_low_hz, sr, 0.707);
            sc_hpf_filters.push(hpf);

            let mut lpf = Biquad::new();
            lpf.set_lpf(dess_band_high_hz, sr, 0.707);
            sc_lpf_filters.push(lpf);

            envelope_states.push(0.0);
        }

        // Attack: 1ms, Release: 50ms
        let attack_coef  = libm::expf(-1.0 / (0.001 * sample_rate as f32));
        let release_coef = libm::expf(-1.0 / (0.050 * sample_rate as f32));

        Self {
            sc_hpf_filters,
            sc_lpf_filters,
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

                // 1. Sidechain bandpass (HPF @ low, LPF @ high)
                let sc_sample = self.sc_hpf_filters[ch].process(sample);
                let sc_sample = self.sc_lpf_filters[ch].process(sc_sample);

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
        let mut process = Stage3DeEss::new(48000, 2, 6000.0, 8000.0);
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
