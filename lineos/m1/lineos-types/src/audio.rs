// src/audio.rs
// Replaces: sp314_dsp::types::audio::AudioChunk
// v3 equivalent: named fields, sample_rate included

use serde::{Deserialize, Serialize};

/// Stereo audio buffer — replaces v2.9 AudioChunk.
/// left and right are interleaved-free f32 sample vectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StereoBuffer {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub sample_rate: u32,
    pub num_frames: usize,
}

impl StereoBuffer {
    pub fn new(sample_rate: u32, num_frames: usize) -> Self {
        Self {
            left: vec![0.0; num_frames],
            right: vec![0.0; num_frames],
            sample_rate,
            num_frames,
        }
    }

    pub fn duration_secs(&self) -> f32 {
        self.num_frames as f32 / self.sample_rate as f32
    }
}

/// v2.9 compatibility alias — allows gradual migration
pub type AudioChunk = StereoBuffer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioPayload {
    Stereo(StereoBuffer),
    FiveDotOne {
        // [L, R, C, LFE, Ls, Rs] — order confirmed from symphonia-core's
        // Channels bitflag iteration order (see commit fa82b00). Array form
        // (not 6 named fields) chosen to match downmix_bs775's existing
        // &[Vec<f32>; 6] signature without extra destructuring at call sites.
        channels: [Vec<f32>; 6],
        sample_rate: u32,
        num_frames: usize,
    },
}
