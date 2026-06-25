//! types.rs — SessionStateJson and supporting types.
//! Mirrors stillair src-tauri/src/commands/session.rs exactly.
//! No core imports (Amendment A-002 §3).

use serde::{Deserialize, Serialize};

// ── Cockpit Tier ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(non_camel_case_types)]
pub enum CockpitTier {
    Tier1_BlackBox,
    Tier2_Medium,
    Tier3_Pro,
}

// ── LoudnessMetricsJson ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoudnessMetricsJson {
    pub integrated_lufs: f32,
    pub short_term_lufs: f32,
    pub momentary_lufs: f32,
    pub true_peak_dbtp: f32,
    pub lra: f32,
    pub k_weighted: bool,
    pub ebu_r128_target_lufs: f32,
    pub ebu_r128_compliant: bool,
    pub spotify_compliant: bool,
    pub youtube_compliant: bool,
    pub apple_music_compliant: bool,
    pub apple_podcasts_compliant: bool,
    pub broadcast_compliant: bool,
    pub tidal_compliant: bool,
}

// ── QualityMetricsJson ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityMetricsJson {
    pub stereo_correlation: f32,
    pub phase_coherence: f32,
    pub stereo_width: f32,
    pub dynamic_range_db: f32,
    pub rms_db: f32,
    pub spectral_centroid: f32,
    pub spectral_flatness: f32,
    pub clips_detected: u32,
    pub clip_free: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BandSpatialJson {
    pub pan_mean: f32,
    pub pan_width: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredSpatialJson {
    pub low: BandSpatialJson,
    pub low_mid: BandSpatialJson,
    pub mid: BandSpatialJson,
    pub high_mid: BandSpatialJson,
    pub high: BandSpatialJson,
}

// ── VisualizationDataJson (Phase 14 — §2 IPC type) ───────────────────────────

/// Precomputed visualization data from backend (get_visualization_data command).
/// UI receives this and renders — computes nothing itself.
/// Authority: UI Agent Context v2.1 §2 · Phase 14 P14-003.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualizationDataJson {
    /// Spectrum waveform — SVG path string, 400×160 viewBox. Closed fill path.
    pub spectrum_svg_path: String,

    /// Lissajous goniometer paths — 120×120 viewBox. Rendered by StereoScope.
    /// Outer orbit: rendered cyan (stereo width orbit).
    pub lissajous_path_outer: String,
    /// Inner orbit: rendered magenta (correlation tightness).
    pub lissajous_path_inner: String,
    /// Detail traces: rendered at low opacity for visual richness.
    pub lissajous_path_detail1: String,
    pub lissajous_path_detail2: String,

    /// Waveform placeholders — Phase 15: real before/after PCM snapshots.
    pub waveform_before_svg: String,
    pub waveform_after_svg: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeFrameJson {
    /// Pre-mastering spectrum (Ghost — raw/before DSP). 64 log-spaced dBFS bands.
    pub spectrum_before: Vec<f32>,
    /// Post-mastering spectrum (Core — after DSP). 64 log-spaced dBFS bands.
    pub spectrum_after: Vec<f32>,
    pub energy_mid: f32,
    pub energy_side: f32,
    pub gonio_path: Vec<[f32; 2]>,
    pub position_ms: u64,
}

// ── ComplianceJson ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceJson {
    pub spotify: bool,
    pub youtube: bool,
    pub apple: bool,
    pub tidal: bool,
    pub broadcast: bool,
    pub ebu_r128: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ZoneFlagsJson {
    pub zone_cymbal_harsh: bool,
    pub zone_sub_rumble: bool,
    pub zone_boxiness: bool,
    pub zone_phase_issue: bool,
    pub zone_harsh_resonance: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct VerificationResultJson {
    pub passed: bool,
    pub trim_applied_db: f32,
    pub was_trimmed: bool,
    pub warning: Option<String>,
}

// ── CoachFindings & Narrative ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueJson {
    pub id: String,
    pub severity: String, // "info" | "low" | "medium" | "high"
    pub current: f32,
    pub target: f32,
    pub delta: f32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoachFindingsJson {
    pub issues: Vec<IssueJson>,
    pub recommendation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingExplanation {
    pub issue_id: String,
    pub severity: String,
    pub title: String,
    pub why: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoachNarrativeJson {
    pub summary: String,
    pub explanations: Vec<FindingExplanation>,
    pub model_used: String,
}

// ── AudioMeta ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioMeta {
    pub path: String,
    pub name: String,
    pub format: String,
    pub sample_rate: u32,
    pub bit_depth: Option<u32>,
    pub duration_s: f64,
    pub channels: u8,
}

// ── SessionStateJson ──────────────────────────────────────────────────────────

/// P9-008: Complete session snapshot — one IPC call replaces the Data Cascade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStateJson {
    pub blob_id: String,
    pub loudness: LoudnessMetricsJson,
    pub quality: QualityMetricsJson,
    pub compliance: ComplianceJson,
    pub findings: CoachFindingsJson,
    #[serde(default)]
    pub spatial: Option<StoredSpatialJson>,
    pub narrative: Option<CoachNarrativeJson>,
    #[serde(default)]
    pub aether_cert: Option<String>,
    #[serde(default)]
    pub aether_persona: Option<String>,
    #[serde(default)]
    pub aether_config: Option<String>,
    #[serde(default)]
    pub zone_flags: Option<ZoneFlagsJson>,
    #[serde(default)]
    pub verification: Option<VerificationResultJson>,
    /// JINI suggestion — personality-aware mastering recommendation (J-P8).
    #[serde(default)]
    pub jini: Option<JiniSuggestionJson>,

    #[serde(default)]
    pub dsp_chain: Option<DspChainStateJson>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize, Default)]
pub struct DspChainStateJson {
    pub eq_active: bool,
    pub comp_active: bool,
    pub sat_active: bool,
    pub limit_active: bool,
}

// ── ExportResult ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportResult {
    pub written_path: String,
    pub format: String,
}

// ── PlaybackStateJson — transport metrics (A-003 §5: no PCM) ─────────────────

/// Position/state from xaak kernel — exposed to Cockpit.
/// No PCM: position_ms / duration_ms / is_playing only (A-003 §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackStateJson {
    pub blob_id: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub is_playing: bool,
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(default = "default_active_ab")]
    pub active_ab: String,
}

fn default_active_ab() -> String {
    "B".to_string()
}

// ── LiveTelemetryJson — live momentary LUFS during playback ───────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveTelemetryJson {
    pub momentary_lufs: f32,
    pub short_term_lufs: f32,
    pub true_peak_dbtp: f32,
    pub position_ms: u64,
}

// ── JINI Suggestion (J-P5 UI layer) ──────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct JiniSuggestionJson {
    pub narrative: String,
    pub action_type: String,  // "macro_change" | "flavour_switch" | "nothing"
    pub action_label: String, // human readable e.g. "Switch to Clean mode"
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum JiniPersonaState {
    Beginner,
    #[default]
    Intermediate,
    Pro,
}
