//! types.rs — SessionStateJson and supporting types.
//! Mirrors stillair src-tauri/src/commands/session.rs exactly.
//! No core imports (Amendment A-002 §3).

use serde::{Deserialize, Serialize};

// ── LoudnessMetricsJson ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoudnessMetricsJson {
    pub integrated_lufs:          f32,
    pub short_term_lufs:          f32,
    pub momentary_lufs:           f32,
    pub true_peak_dbtp:           f32,
    pub lra:                      f32,
    pub k_weighted:               bool,
    pub ebu_r128_target_lufs:     f32,
    pub ebu_r128_compliant:       bool,
    pub spotify_compliant:        bool,
    pub youtube_compliant:        bool,
    pub apple_music_compliant:    bool,
    pub apple_podcasts_compliant: bool,
    pub broadcast_compliant:      bool,
    pub tidal_compliant:          bool,
}

// ── QualityMetricsJson ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityMetricsJson {
    pub stereo_correlation: f32,
    pub phase_coherence:    f32,
    pub stereo_width:       f32,
    pub dynamic_range_db:   f32,
    pub rms_db:             f32,
    pub spectral_centroid:  f32,
    pub spectral_flatness:  f32,
    pub clips_detected:     u32,
    pub clip_free:          bool,
}

// ── ComplianceJson ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceJson {
    pub spotify:   bool,
    pub youtube:   bool,
    pub apple:     bool,
    pub tidal:     bool,
    pub broadcast: bool,
    pub ebu_r128:  bool,
}

// ── CoachFindings & Narrative ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueJson {
    pub id:       String,
    pub severity: String,  // "info" | "low" | "medium" | "high"
    pub current:  f32,
    pub target:   f32,
    pub delta:    f32,
    pub tags:     Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoachFindingsJson {
    pub issues:         Vec<IssueJson>,
    pub recommendation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingExplanation {
    pub issue_id:   String,
    pub severity:   String,
    pub title:      String,
    pub why:        String,
    pub suggestion: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoachNarrativeJson {
    pub summary:      String,
    pub explanations: Vec<FindingExplanation>,
    pub model_used:   String,
}

// ── AudioMeta ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioMeta {
    pub path:        String,
    pub name:        String,
    pub format:      String,
    pub sample_rate: u32,
    pub bit_depth:   Option<u32>,
    pub duration_s:  f64,
    pub channels:    u8,
}

// ── SessionStateJson ──────────────────────────────────────────────────────────

/// P9-008: Complete session snapshot — one IPC call replaces the Data Cascade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStateJson {
    pub blob_id:    String,
    pub loudness:   LoudnessMetricsJson,
    pub quality:    QualityMetricsJson,
    pub compliance: ComplianceJson,
    pub findings:   CoachFindingsJson,
    pub narrative:  Option<CoachNarrativeJson>,
}

// ── ExportResult ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportResult {
    pub written_path: String,
    pub format:       String,
}
