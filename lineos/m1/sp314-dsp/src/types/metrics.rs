//! Quality metrics — canonical EBU R128 BS.1770-4 measurements.
//! These are produced by AnalysisAccumulator and embedded in GoldenBlob.
//! Downstream modules read from these — they never re-measure.
//! Authority: LineOS Constitution v2.0 §07 (comparator rule)

#[derive(Debug, Clone)]
pub struct QualityMetrics {
    /// EBU R128 integrated loudness (LUFS)
    pub integrated_lufs:     f32,
    /// True peak ceiling (dBFS)
    pub true_peak_dbfs:      f32,
    /// Loudness range (LU) — placeholder for Phase 3 (EBU R128 Level 2+)
    pub loudness_range_lu:   f32,
    /// Canonical BS.1770-4 integrated loudness
    pub bs1770_integrated:   f32,
    /// Canonical BS.1770-4 true peak
    pub bs1770_true_peak:    f32,
    /// Stereo correlation coefficient [0.0, 1.0]
    pub stereo_correlation:  f32,
    /// DC offset (informational)
    pub dc_offset:           f32,
}

impl Default for QualityMetrics {
    fn default() -> Self {
        Self {
            integrated_lufs:    f32::NEG_INFINITY,
            true_peak_dbfs:     f32::NEG_INFINITY,
            loudness_range_lu:  0.0,
            bs1770_integrated:  f32::NEG_INFINITY,
            bs1770_true_peak:   f32::NEG_INFINITY,
            stereo_correlation: 1.0,
            dc_offset:          0.0,
        }
    }
}
