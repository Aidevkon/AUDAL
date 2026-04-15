//! Frontend-local types — WASM-safe, fully typed, no serde_json::Value.
//! Authority: Phase 6 P6-005/P6-007 · golden-blob-spec.md v1.0
//!
//! These are WASM-safe re-definitions of backend types.
//! FORBIDDEN: Importing lineos-rule-engine directly.
//! FORBIDDEN: Binary audio bytes in any type.
//! FORBIDDEN: serde_json::Value crossing the WASM boundary.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

// ── AudioMeta ─────────────────────────────────────────────────────────────────

/// File metadata displayed in FM1 Session panel.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioMeta {
    /// Full filesystem path — forwarded to trigger_mastering.
    pub path:        String,
    pub name:        String,
    pub format:      String,
    pub sample_rate: u32,
    pub bit_depth:   u8,
    pub duration_s:  f32,
    pub channels:    u8,
}

// ── GoldenBlobJson — IPC contract ─────────────────────────────────────────────
//
// Mirrors ipc/m0_client.rs GoldenBlobJson exactly.
// Authority: golden-blob-spec.md v1.0 §Structure
// All fields typed — no serde_json::Value.

/// Golden Blob received from Tauri backend as JSON.
/// No audio bytes — metrics only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldenBlobJson {
    pub id:               String,
    pub version:          String,
    #[serde(rename = "type")]
    pub blob_type:        String,
    pub created_at:       String,
    pub input_hash:       String,
    pub seed:             u64,
    pub pipeline_version: String,
    pub preset_id:        String,
    pub loudness:         LoudnessMetricsJson,
    pub quality:          QualityMetricsJson,
    pub provenance:       ProvenanceJson,
}

/// BS.1770-4 loudness + platform compliance flags.
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

/// Objective quality measurements.
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

/// Audit trail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceJson {
    pub engine_id:          String,
    pub engine_version:     String,
    pub processing_time_ms: u64,
    pub host_os:            String,
    pub created_by:         String,
    pub aether_enriched:    bool,
    pub aether_devices:     Vec<String>,
}

// ── CoachFindings — from evaluate_findings Tauri command ──────────────────────

/// Coach findings received from evaluate_findings Tauri command.
#[derive(Debug, Clone, PartialEq)]
pub struct CoachFindings {
    pub issues:         Vec<Issue>,
    pub recommendation: String,
}

/// A single rule-engine finding.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub id:       String,
    pub severity: Severity,
    pub current:  f32,
    pub target:   f32,
    pub delta:    f32,
    pub tags:     Vec<String>,
}

/// Severity level — maps to design token colors per design-tokens-v1.0.md §5.
#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    High,
    Medium,
    Low,
    Info,
}

impl Severity {
    /// CSS class suffix for severity badge — design-tokens-v1.0.md §5.
    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::High   => "badge-high",
            Self::Medium => "badge-medium",
            Self::Low    => "badge-low",
            Self::Info   => "badge-info",
        }
    }

    /// Display name for UI text nodes.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::High   => "HIGH",
            Self::Medium => "MEDIUM",
            Self::Low    => "LOW",
            Self::Info   => "INFO",
        }
    }

    /// CSS class for finding card border — design-tokens-v1.0.md §5.
    pub fn card_class(&self) -> &'static str {
        match self {
            Self::High   => "sev-high",
            Self::Medium => "sev-medium",
            Self::Low    => "sev-low",
            Self::Info   => "sev-info",
        }
    }

    /// From the string returned by evaluate_findings (lowercase).
    pub fn from_str(s: &str) -> Self {
        match s {
            "high"   => Self::High,
            "medium" => Self::Medium,
            "low"    => Self::Low,
            _        => Self::Info,
        }
    }
}

// ── Metrics — derived from GoldenBlobJson for InsightsPanel display ───────────

/// Flattened metrics for InsightsPanel display.
/// Derived from GoldenBlobJson — never re-measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Metrics {
    pub integrated_lufs:    f32,
    pub true_peak_dbtp:     f32,
    pub lra:                f32,
    pub dynamic_range_db:   f32,
    pub stereo_correlation: f32,
    // Compliance flags
    pub spotify_compliant:        bool,
    pub youtube_compliant:        bool,
    pub apple_music_compliant:    bool,
    pub apple_podcasts_compliant: bool,
    pub broadcast_compliant:      bool,
    pub tidal_compliant:          bool,
}

impl Metrics {
    /// Construct from GoldenBlobJson — P6-005 Data Cascade.
    pub fn from_blob(blob: &GoldenBlobJson) -> Self {
        Self {
            integrated_lufs:    blob.loudness.integrated_lufs,
            true_peak_dbtp:     blob.loudness.true_peak_dbtp,
            lra:                blob.loudness.lra,
            dynamic_range_db:   blob.quality.dynamic_range_db,
            stereo_correlation: blob.quality.stereo_correlation,
            spotify_compliant:        blob.loudness.spotify_compliant,
            youtube_compliant:        blob.loudness.youtube_compliant,
            apple_music_compliant:    blob.loudness.apple_music_compliant,
            apple_podcasts_compliant: blob.loudness.apple_podcasts_compliant,
            broadcast_compliant:      blob.loudness.broadcast_compliant,
            tidal_compliant:          blob.loudness.tidal_compliant,
        }
    }

    /// Phase 5 stub — used when M0 is not running.
    pub fn stub() -> Self {
        Self {
            integrated_lufs:    -14.0,
            true_peak_dbtp:     -1.0,
            lra:                 8.0,
            dynamic_range_db:    9.5,
            stereo_correlation:  0.94,
            spotify_compliant:        true,
            youtube_compliant:        true,
            apple_music_compliant:    false,
            apple_podcasts_compliant: false,
            broadcast_compliant:      false,
            tidal_compliant:          true,
        }
    }
}

// ── TelemetrySignal — live FM2 progress ──────────────────────────────────────

/// Live telemetry event from Tauri during FM2 mastering.
/// Emitted every ~100ms. Drives InsightsPanel live updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySignal {
    pub lufs:     f32,     // Current integrated LUFS (live)
    pub peak:     f32,     // Current true peak dBTP (live)
    pub progress: f32,     // 0.0–1.0 pipeline progress
    pub stage:    String,  // e.g. "EQ", "Compress", "Limit"
}

// ── IssueParams (retained for Phase 5 compatibility) ─────────────────────────

/// Numeric params for an issue (Phase 5 compat — same fields as IssueJson).
#[derive(Debug, Clone, PartialEq)]
pub struct IssueParams {
    pub current: f32,
    pub target:  f32,
    pub delta:   f32,
}
