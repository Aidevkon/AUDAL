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
        }
    }
}

/// F-050: RAII guard for raw and mastered PCM dumps on /tmp.
///
/// The dumps' lifetime binds to their readers via `Arc<ManagedPcm>`:
/// every `blob_store.get()` / `PcmTransfer` clone extends the guard,
/// and the file is deleted only when the last Arc drops.
///
/// The startup sweep in lib.rs (F-050α) acts as the crash backstop —
/// it buries corpses from killed/crashed sessions that never got a
/// clean Drop. This RAII guard handles the living-file lifecycle.
///
/// NOT Clone: a Drop owner must never be Clone — Arc<ManagedPcm>
/// does all sharing. Cloning would create two owners each trying
/// to delete the same file on drop.
#[derive(Debug)]
pub struct ManagedPcm(std::path::PathBuf);

impl ManagedPcm {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self(path)
    }
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Default for ManagedPcm {
    fn default() -> Self {
        Self(std::path::PathBuf::new()) // empty path, deletes nothing
    }
}

impl Drop for ManagedPcm {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}
