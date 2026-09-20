#[derive(Debug, Clone)]
pub struct StreamingPlan {
    pub audio_path: String,
    pub preset_id: String,
    pub flavour_id: Option<String>,
    pub intent_tone: Option<f32>,
    pub intent_dynamics: Option<f32>,
    /// Album cohesion override: when Some, this LUFS target wins
    /// over the preset's default (LoudnessTarget::from_preset).
    /// None for every normal single-track master today — album
    /// cohesion is the only intended caller, wired separately
    /// (Part 2b).
    pub target_lufs_override: Option<f32>,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct StreamingOutput {
    pub job_id: String,
    pub blob_id: String,
    pub status: &'static str,
    pub pcm_data: Option<std::sync::Arc<crate::audio::ManagedPcm>>,
    pub num_frames: usize,
    pub sample_rate: u32,
    /// Real content hash of the mastered output (from the C1
    /// measured wav→raw pass) — was previously unavailable to
    /// callers, forcing album cohesion to substitute session_id as
    /// a fake input_hash (found 2026-07-18).
    pub pcm_blake3: String,
    /// Actual POST-mastering integrated LUFS — was previously
    /// unavailable to callers, forcing album cohesion to report the
    /// PRE-mastering measurement instead (found 2026-07-18).
    pub output_lufs: f32,
    /// F-050: raw dump guard — cloned into PcmTransfer for xaak A/B.
    pub raw_pcm_data: Option<std::sync::Arc<crate::audio::ManagedPcm>>,
}

#[derive(Debug, Clone)]
pub enum ExecutorError {
    DspFailed(String),
    BlobStoreFailed(String),
}
