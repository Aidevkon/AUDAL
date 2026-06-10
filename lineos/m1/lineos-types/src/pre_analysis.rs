// pre_analysis.rs — PreAnalysisData types and constants
// Authority: Pre-Analysis Constitution v1.3 §3, §5
// Oracle:    tests/fixtures/generate_pre_analysis_fixture.py

use serde::{Deserialize, Serialize};

// ── Zone Threshold Constants (Constitution §5) ───────────────────────────────

pub const ZONE_SUB_RUMBLE_THRESHOLD_DB: f32 = -30.0;
pub const ZONE_CYMBAL_HARSH_RMS_DB: f32 = -18.0;
pub const ZONE_CYMBAL_HARSH_CREST_DB: f32 = 8.0;
pub const ZONE_BOXINESS_RMS_DB: f32 = -20.0;
pub const ZONE_BOXINESS_LRA_LU: f32 = 5.0;
pub const ZONE_PHASE_ISSUE_CORRELATION: f32 = 0.3;

// ── Analysis Constants ───────────────────────────────────────────────────────

/// Minimum input length for meaningful analysis. Shorter inputs
/// return PreAnalysisData::silent(). Matches FFT_SIZE in sp314-dsp.
pub const MINIMUM_ANALYSIS_SAMPLES: usize = 2048;

/// ±20 bins for local moving average in resonant peak detection.
pub const RESONANT_PEAK_WINDOW_BINS: usize = 20;

/// Bin must have ≥ 0.1% of peak bin magnitude to be considered
/// a resonance candidate. Prevents noise-floor artifacts.
/// -60 dB below peak magnitude.
pub const RESONANT_PEAK_MAG_FLOOR_RATIO: f32 = 1e-3;

/// Maximum number of resonant peaks reported. (INV-PA-12)
pub const RESONANT_PEAK_MAX_COUNT: usize = 16;

/// Resonant peak statistical threshold: bin > local_avg + 3σ.
pub const RESONANT_PEAK_SIGMA: f32 = 3.0;

// ── Zone Activation Flags ────────────────────────────────────────────────────

/// Deterministic zone activation flags (Constitution §5).
/// Each flag is a boolean derived from threshold comparisons
/// against spectral profile, crest factor, LRA, and phase correlation.
/// No ML, no randomness. (INV-PA-7)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ZoneActivationFlags {
    pub zone_cymbal_harsh: bool,
    pub zone_sub_rumble: bool,
    pub zone_boxiness: bool,
    pub zone_phase_issue: bool,
    pub zone_harsh_resonance: bool,
}

// ── PreAnalysisData ──────────────────────────────────────────────────────────

/// Complete pre-analysis output (Constitution §3).
/// Computed BEFORE NMF and BEFORE Aether, from raw stereo PCM.
/// Same input = same output always. (INV-PA-1)
///
/// Band indices for `spectral_profile_db` and `band_phase_correlation`:
///   [0] Sub      20–80 Hz
///   [1] Bass     80–250 Hz
///   [2] LowMid   250–500 Hz
///   [3] Mid      500–2000 Hz
///   [4] HighMid  2000–8000 Hz
///   [5] Air      8000–20000 Hz
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreAnalysisData {
    // ── Dynamics Baseline ──
    pub integrated_lufs: f32,
    pub true_peak_dbtp: f32,
    pub loudness_range: f32,
    pub dynamic_range_db: f32,
    pub global_crest_factor_db: f32,

    // ── Tonal Balance (6-band RMS in dBFS) ──
    pub spectral_profile_db: [f32; 6],

    // ── Spectral Shape ──
    pub spectral_rolloff_hz: f32,

    // ── Transient Profile ──
    pub transient_density: f32,

    // ── Spatial Health ──
    pub global_phase_correlation: f32,
    pub side_mid_ratio_db: f32,
    pub stereo_width: f32,

    // ── Per-Band Phase Correlation (same 6 bands) ──
    pub band_phase_correlation: [f32; 6],

    // ── Resonant Peak Flags (Hz, sorted ascending, max 16) ──
    pub resonant_peaks_hz: Vec<f32>,

    // ── Zone Activation ──
    pub zone_flags: ZoneActivationFlags,
}

impl PreAnalysisData {
    /// Safe default for empty/short inputs. (INV-PA-13)
    /// All loudness at silence floor, correlations at mono-coherent,
    /// all zone flags false, no resonant peaks.
    pub fn silent() -> Self {
        Self {
            integrated_lufs: -144.0,
            true_peak_dbtp: -144.0,
            loudness_range: 0.0,
            dynamic_range_db: 0.0,
            global_crest_factor_db: 0.0,
            spectral_profile_db: [-144.0; 6],
            spectral_rolloff_hz: 0.0,
            transient_density: 0.0,
            global_phase_correlation: 1.0,
            side_mid_ratio_db: -60.0,
            stereo_width: 0.0,
            band_phase_correlation: [1.0; 6],
            resonant_peaks_hz: vec![],
            zone_flags: ZoneActivationFlags::default(),
        }
    }
}
