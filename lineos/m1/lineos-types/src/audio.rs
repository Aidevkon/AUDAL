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
    /// 5-stem payload for intelligent spatial
    /// upmix via StemChannelAssignments.
    /// Produced by NMF HPSS pipeline.
    /// voice = isolated vocals/lead
    /// drums = percussive transients
    /// bass = low-frequency content
    /// harmonics = melodic/harmonic content
    /// ambience = reverb tails / room
    Stems {
        voice: StereoBuffer,
        drums: StereoBuffer,
        bass: StereoBuffer,
        harmonics: StereoBuffer,
        ambience: StereoBuffer,
        sample_rate: u32,
        num_frames: usize,
    },
}

impl AudioPayload {
    /// Downmix to stereo for telemetry/analysis only.
    /// Does NOT modify the original payload.
    /// Uses ITU-R BS.775 coefficients for 5.1 → stereo.
    ///
    /// BS.775 matrix (standard coefficients):
    ///   L_out  = L + 0.707*C + 0.707*Ls
    ///   R_out  = R + 0.707*C + 0.707*Rs
    ///   (LFE excluded — not part of loudness measurement)
    pub fn to_stereo_for_telemetry(&self) -> StereoBuffer {
        match self {
            AudioPayload::Stereo(buf) => buf.clone(),
            AudioPayload::FiveDotOne {
                channels,
                sample_rate,
                num_frames,
            } => {
                // channels: [L, R, C, LFE, Ls, Rs]
                // index:     0  1  2   3   4   5
                const C: f32 = 0.707;
                let left: Vec<f32> = (0..*num_frames)
                    .map(|i| channels[0][i] + C * channels[2][i] + C * channels[4][i])
                    .collect();
                let right: Vec<f32> = (0..*num_frames)
                    .map(|i| channels[1][i] + C * channels[2][i] + C * channels[5][i])
                    .collect();
                StereoBuffer {
                    left,
                    right,
                    sample_rate: *sample_rate,
                    num_frames: *num_frames,
                }
            }
            AudioPayload::Stems {
                voice,
                drums,
                bass,
                harmonics,
                ambience,
                sample_rate,
                num_frames,
            } => {
                // Master bus sum: όλα τα stems
                // για telemetry downmix
                let left: Vec<f32> = (0..*num_frames)
                    .map(|i| {
                        voice.left[i]
                            + drums.left[i]
                            + bass.left[i]
                            + harmonics.left[i]
                            + ambience.left[i]
                    })
                    .collect();
                let right: Vec<f32> = (0..*num_frames)
                    .map(|i| {
                        voice.right[i]
                            + drums.right[i]
                            + bass.right[i]
                            + harmonics.right[i]
                            + ambience.right[i]
                    })
                    .collect();
                StereoBuffer {
                    left,
                    right,
                    sample_rate: *sample_rate,
                    num_frames: *num_frames,
                }
            }
        }
    }
}
