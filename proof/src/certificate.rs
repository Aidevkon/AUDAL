// proof/src/certificate.rs — ExecutionCertificate + VerificationError
// Authority: spec/locked/S-010_execution_proof.md v1.0

/// Cryptographic proof of a mastering render.
/// All hashes are SHA-256 hex strings (64 characters).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionCertificate {
    pub version: String, // "1.0"
    /// SHA-256 of input PCM (raw f32 big-endian bytes)
    pub input_pcm_hash: String,
    /// SHA-256 of PersonaConfig JSON (canonical, sorted keys)
    pub persona_hash: String,
    /// SHA-256 of Intent JSON (canonical). "none" if not recorded.
    pub intent_hash: String,
    /// SHA-256 of compound string "project_id:track_id:persona_id"
    /// Matches ChaosEngine::build_seed() input (S-006).
    pub chaos_seed_hash: String,
    /// SHA-256 of ZoneAdjustments JSON (canonical)
    pub zone_resolutions_hash: String,
    /// SHA-256 of DspConfig JSON (canonical, sorted keys)
    /// DspConfig passed directly to generate() — not from ProofLog.
    pub final_dsp_config_hash: String,
    /// SHA-256 of output PCM (raw f32 big-endian bytes)
    pub output_pcm_hash: String,
    pub rendered_at: String, // ISO-8601
    pub system_version: String,
    pub persona_id: String,
    pub preset_name: String,
}

#[derive(Debug)]
pub enum VerificationError {
    InputPcmMismatch { expected: String, actual: String },
    PersonaMismatch { expected: String, actual: String },
    DspConfigMismatch { expected: String, actual: String },
    OutputPcmMismatch { expected: String, actual: String },
}
