//! Frontend-local types mirroring lineos-rule-engine CoachFindings.
//! These are WASM-safe re-definitions — NOT importing lineos-rule-engine directly.
//! Per Phase 5 master-prompt: "No UI component importing lineos-rule-engine directly."
//!
//! In Phase 6, Tauri backend will deserialize real CoachFindings from M0
//! and send typed structs over IPC. These types must match that contract.

use serde::{Deserialize, Serialize};

/// Severity of a coach finding — matches lineos-rule-engine Severity enum.
/// Values must serialize as lowercase strings per coach-findings.schema.json.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info   => "info",
            Self::Low    => "low",
            Self::Medium => "medium",
            Self::High   => "high",
        }
    }
    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Info   => "badge-info",
            Self::Low    => "badge-low",
            Self::Medium => "badge-medium",
            Self::High   => "badge-high",
        }
    }
    pub fn card_class(&self) -> &'static str {
        match self {
            Self::Info   => "sev-info",
            Self::Low    => "sev-low",
            Self::Medium => "sev-medium",
            Self::High   => "sev-high",
        }
    }
}

/// Typed numeric parameters — no serde_json::Value.
/// Matches IssueParams in coach-findings.schema.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueParams {
    pub current: f32,
    pub target:  f32,
    pub delta:   f32,
}

/// A single coach finding.
/// Matches Issue in coach-findings.schema.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub id:       String,
    pub severity: Severity,
    pub params:   IssueParams,
    pub tags:     Vec<String>,
}

/// Output of the rule-engine evaluation.
/// Received from Tauri backend via IPC in Phase 6.
/// In Phase 5: stub-constructed in app.rs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoachFindings {
    pub issues:         Vec<Issue>,
    pub recommendation: String,
}

impl CoachFindings {
    pub fn has_blocking(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::High)
    }
}

/// Audio file metadata returned from load_audio_file Tauri command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMeta {
    pub name:        String,
    pub format:      String,
    pub sample_rate: u32,
    pub bit_depth:   u8,
    pub duration_s:  f32,
    pub channels:    u8,
}

/// EBU R128 metrics stub — real data arrives from M0 IPC in Phase 6.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub lufs_integrated:   f32,
    pub true_peak:         f32,
    pub loudness_range:    f32,
    pub dynamic_range:     f32,
    pub stereo_correlation: f32,
}

impl Metrics {
    /// Phase 5 stub metrics matching the lufs_compliance stub finding.
    pub fn stub() -> Self {
        Self {
            lufs_integrated:    -12.0,
            true_peak:           -1.5,
            loudness_range:       8.0,
            dynamic_range:        9.5,
            stereo_correlation:   0.94,
        }
    }
}
