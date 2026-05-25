// src/metrics.rs
// Replaces: sp314_dsp::types::metrics::Ebu128Measurement

use serde::{Deserialize, Serialize};

/// ITU-R BS.1770-4 / EBU R128 loudness measurement.
/// Replaces v2.9 Ebu128Measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LufsReport {
    /// Integrated loudness in LUFS (ITU-R BS.1770-4)
    pub integrated_lufs:  f32,
    /// True peak in dBFS
    pub true_peak_dbfs:   f32,
    /// Loudness Range in LU (EBU R128)
    pub loudness_range_lu: f32,
    /// Short-term loudness in LUFS (last 3s)
    pub short_term_lufs:  Option<f32>,
}

/// v2.9 compatibility alias
pub type Ebu128Measurement = LufsReport;
