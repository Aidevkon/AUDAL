//! AnalysisReport — input contract for lineos-rule-engine.
//! Built from Golden Blob QualityMetrics + InsightsReport compliance flags.
//! Phase 4 scope: QualityMetrics + ComplianceFlags only.
//! SpectralMetrics and DynamicsMetrics are Phase 5+.
//! Schema: lineos/shared/schema/analysis-report.schema.json
//! Authority: LineOS Constitution v2.0 §07 · Coach-Core README §4.1

use serde::{Deserialize, Serialize};

/// Input to the rule-engine — canonical facts assembled from telemetry + insights.
/// Never contains raw audio. Never mutable after construction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisReport {
    pub quality: QualityMetrics,
    pub compliance: ComplianceFlags,
    pub version: String,
}

/// Measurement values from Golden Blob (via telemetry + sp314-dsp).
/// Read-only facts — rule-engine never computes these itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityMetrics {
    /// BS.1770-4 integrated gated loudness (LUFS)
    pub lufs_integrated: f32,
    /// Short-term loudness — 3s window (EBU R128 §2.3)
    pub lufs_short_term: f32,
    /// Momentary loudness — 400ms window (EBU R128 §2.2)
    pub lufs_momentary: f32,
    /// BS.1770-4 true peak (dBTP)
    pub true_peak: f32,
    /// EBU R128 LRA (LU) — from lineos-telemetry
    pub loudness_range: f32,
    /// Stereo correlation [-1.0, 1.0]
    pub stereo_correlation: f32,
    /// Peak-to-RMS dynamic range (dB)
    pub dynamic_range: f32,
    /// DC offset [-1.0, 1.0]
    pub dc_offset: f32,
}

/// Per-platform compliance booleans — from InsightsReport::preset_results.
/// Pure pass/fail flags; rule-engine re-evaluates conditions independently
/// to produce parameterised Issues with severity and delta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComplianceFlags {
    pub spotify_ok: bool,
    pub youtube_ok: bool,
    pub apple_ok: bool,
    pub tidal_ok: bool,
    pub broadcast_ok: bool,
}

impl AnalysisReport {
    /// Canonical constructor. Stamps version "1.0".
    pub fn from_metrics(quality: QualityMetrics, compliance: ComplianceFlags) -> Self {
        Self {
            quality,
            compliance,
            version: "1.0".to_string(),
        }
    }
}

impl ComplianceFlags {
    /// All presets fail — convenience helper for tests.
    pub fn all_fail() -> Self {
        Self {
            spotify_ok: false,
            youtube_ok: false,
            apple_ok: false,
            tidal_ok: false,
            broadcast_ok: false,
        }
    }

    /// All presets pass — convenience helper for tests.
    pub fn all_pass() -> Self {
        Self {
            spotify_ok: true,
            youtube_ok: true,
            apple_ok: true,
            tidal_ok: true,
            broadcast_ok: true,
        }
    }
}
