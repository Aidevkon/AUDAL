use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "900";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusEnvelope {
    pub protocol_version: String,  // always "900"
    pub session_id:       String,  // = GoldenBlob blob_id (sha256)
    pub stems:            Vec<StemTimeline>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StemTimeline {
    pub stem_type: String,          // "voice"|"drums"|"bass"|"harmonics"|"ambience"
    pub events:    Vec<TimelineEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub state:       String,        // "silence"|"breath"|"consonant"|"vowel"|"tail"
    pub start_ms:    u32,
    pub end_ms:      u32,
    pub duration_ms: u32,
    pub confidence:  f32,           // [0.0, 1.0] — INV-CP-6
    pub session_id:  String,        // INV-CP-9: cross-file bleed guard
    pub attributes:  EnrichedAttributes,
    pub risk:        RiskFlags,
    pub domain:      DomainHint,
    /// 13 MFCC coefficients — timbral fingerprint for this window.
    /// [0.0; 13] for backward compatibility when not computed.
    #[serde(default)]
    pub mfcc:        [f32; 13],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedAttributes {
    pub rms_db:            f32,
    pub crest_factor_db:   f32,
    pub transient_density: f32,
    pub spectral_centroid: f32,
    pub lufs_integrated:   f32,
    pub spectral_flatness: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFlags {
    pub artifact_risk:  f32,   // [0.0, 1.0]
    pub sibilance_risk: f32,
    pub phase_issue:    f32,
    pub sub_rumble:     f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainHint {
    pub stem:         String,   // same as StemTimeline.stem_type
    pub profile_hint: String,   // "podcast_v1"|"music_v1"|"unknown"
}
