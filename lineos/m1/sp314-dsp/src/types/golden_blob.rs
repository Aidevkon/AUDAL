//! GoldenBlob — the canonical output artifact of sp314-dsp.
//! Contract: creator-os/contracts/golden-blob.schema.json
//! Authority: LineOS Constitution v2.0 §05.3
//! The GoldenBlob is immutable once written. All downstream modules read from it.

use alloc::vec::Vec;
use super::metrics::QualityMetrics;
use super::warnings::WarningRecord;

/// Input profile snapshot recorded at pipeline entry.
/// Mirrors `pipeline::input_profile::InputProfile` — kept in `types/` to avoid
/// a cycle between the `types` and `pipeline` layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldenInputProfile {
    Normal,
    Silence,
    Clipped,
    HighDynamic,
    DCOnly,
    MonoInStereo,
}

#[derive(Debug, Clone)]
pub enum BlobType {
    Audio,
    Av,
}

/// The canonical audio output artifact produced by MasteringPipeline.
///
/// Phase 2 note: `flac_bytes` contains raw PCM bytes until Phase 3 adds
/// symphonia FLAC encoding. The contract shape is final.
#[derive(Debug, Clone)]
pub struct GoldenBlob {
    pub blob_type:       BlobType,
    /// Mastered audio output (FLAC in Phase 3+, raw PCM bytes in Phase 2)
    pub flac_bytes:      Vec<u8>,
    pub quality_metrics: QualityMetrics,
    /// Deterministic seed used for dither — must be preserved for reproducibility
    pub seed:            u64,
    /// SHA-256 of raw input audio — for audit trail
    pub input_hash:      [u8; 32],
    /// Aggregated non-fatal pipeline warnings (v2.9 `WarningAggregator` snapshot)
    pub warnings:        Vec<WarningRecord>,
    /// Input profile detected at pipeline entry (v2.9 §Input Profile Detection)
    pub input_profile:   GoldenInputProfile,
}
