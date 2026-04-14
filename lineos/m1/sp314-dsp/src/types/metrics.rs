//! Quality metrics — canonical EBU R128 BS.1770-4 measurements.
//! Produced by AnalysisAccumulator + embedded in GoldenBlob.
//! Downstream services read from these — they NEVER re-measure raw audio.
//! Authority: LineOS Constitution v2.0 §07 (comparator rule)

/// Core quality metrics embedded in GoldenBlob (produced by sp314-dsp).
/// These are the Phase 2 canonical values. Phase 3 telemetry service
/// extends them with LRA, momentary, and short-term via Ebu128Measurement.
#[derive(Debug, Clone)]
pub struct QualityMetrics {
    /// EBU R128 integrated loudness (LUFS)
    pub integrated_lufs:     f32,
    /// True peak ceiling (dBFS)
    pub true_peak_dbfs:      f32,
    /// Loudness range (LU) — placeholder for Phase 3 LRA (filled by telemetry)
    pub loudness_range_lu:   f32,
    /// Canonical BS.1770-4 integrated loudness
    pub bs1770_integrated:   f32,
    /// Canonical BS.1770-4 true peak
    pub bs1770_true_peak:    f32,
    /// Stereo correlation coefficient [-1.0, 1.0]
    pub stereo_correlation:  f32,
    /// DC offset (informational)
    pub dc_offset:           f32,
    /// Sample rate of the processed audio (Hz) — needed by telemetry
    pub sample_rate:         u32,
    /// Channel count — needed by telemetry
    pub channels:            u16,
    /// Peak-to-RMS dynamic range (dB) — informational
    pub dynamic_range_db:    f32,
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
            sample_rate:        48000,
            channels:           2,
            dynamic_range_db:   0.0,
        }
    }
}

/// Full EBU R128 measurement — produced by lineos-telemetry service.
/// Extends GoldenBlob's QualityMetrics with LRA, momentary, short-term.
/// Reads from Golden Blob only — never touches raw input audio.
/// Authority: LineOS Constitution v2.0 §07 (comparator rule)
/// Schema: lineos/shared/schema/ebu-r128.schema.json
#[derive(Debug, Clone)]
pub struct Ebu128Measurement {
    /// BS.1770-4 integrated gated loudness — from Golden Blob (not re-measured)
    pub integrated_lufs:    f32,
    /// BS.1770-4 true peak — from Golden Blob (not re-measured)
    pub true_peak_dbtp:     f32,
    /// EBU R128 LRA: 95th − 10th percentile of short-term loudness (computed by telemetry)
    pub loudness_range_lu:  f32,
    /// Momentary loudness: last 400ms window (computed by telemetry)
    pub momentary_lufs:     f32,
    /// Short-term loudness: last 3s window (computed by telemetry)
    pub short_term_lufs:    f32,
    /// Stereo correlation — from Golden Blob
    pub stereo_correlation: f32,
    /// Dynamic range — from Golden Blob
    pub dynamic_range_db:   f32,
    /// Sample rate of original mastered output
    pub sample_rate:        u32,
    /// Channel count
    pub channels:           u16,
    /// Duration of the mastered audio in seconds
    pub duration_seconds:   f32,
}

impl Default for Ebu128Measurement {
    fn default() -> Self {
        Self {
            integrated_lufs:    f32::NEG_INFINITY,
            true_peak_dbtp:     f32::NEG_INFINITY,
            loudness_range_lu:  0.0,
            momentary_lufs:     f32::NEG_INFINITY,
            short_term_lufs:    f32::NEG_INFINITY,
            stereo_correlation: 1.0,
            dynamic_range_db:   0.0,
            sample_rate:        48000,
            channels:           2,
            duration_seconds:   0.0,
        }
    }
}
