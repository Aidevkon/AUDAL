// analysis/features.rs — S-002 StemFeatures types and constants
// Authority: spec/locked/S-002_stem_feature_analyzer.md v1.1

pub const ANALYSIS_FFT_SIZE:      usize = 2048;
pub const ANALYSIS_HOP_SIZE:      usize = 512;
pub const ANALYSIS_SAMPLE_RATE:   u32   = 48_000;
pub const LRA_BLOCK_MS:           u32   = 400;
pub const LRA_HOP_MS:             u32   = 100;
pub const DYNAMIC_RANGE_BLOCK_MS: u32   = 100;
pub const ENERGY_RATIO_EPSILON:   f32   = 1e-4;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StemFeatures {
    pub bass:      StemMetrics,
    pub harmonics: StemMetrics,
    pub drums:     StemMetrics,
    pub ambience:  StemMetrics,
    pub mix:       MixMetrics,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StemMetrics {
    pub spectral_centroid_hz:  f32,
    pub spectral_flatness:     f32,
    pub spectral_crest_factor: f32,
    pub integrated_lufs:       f32,
    pub true_peak_dbtp:        f32,
    pub loudness_range:        f32,
    pub rms_db:                f32,
    pub stereo_correlation:    f32,
    pub stereo_width:          f32,
    pub crest_factor_db:       f32,
    pub dynamic_range_db:      f32,
    pub energy_ratio:          f32,
}

impl Default for StemMetrics {
    fn default() -> Self {
        Self {
            spectral_centroid_hz:  1000.0,
            spectral_flatness:     0.5,
            spectral_crest_factor: 10.0,
            integrated_lufs:       -144.0,
            true_peak_dbtp:        -144.0,
            loudness_range:        0.0,
            rms_db:                -144.0,
            stereo_correlation:    1.0,
            stereo_width:          0.0,
            crest_factor_db:       10.0,
            dynamic_range_db:      0.0,
            energy_ratio:          0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MixMetrics {
    pub integrated_lufs:       f32,
    pub true_peak_dbtp:        f32,
    pub loudness_range:        f32,
    pub stereo_correlation:    f32,
    pub stereo_width:          f32,
    pub dynamic_range_db:      f32,
    pub stem_energy_ratios:    [f32; 4],  // [bass, harmonics, drums, ambience]
    /// Energy-weighted average of stem centroids (S-008 requirement)
    pub spectral_centroid_hz:  f32,
}

impl Default for MixMetrics {
    fn default() -> Self {
        Self {
            integrated_lufs:    -144.0,
            true_peak_dbtp:     -144.0,
            loudness_range:     0.0,
            stereo_correlation: 1.0,
            stereo_width:       0.0,
            dynamic_range_db:   0.0,
            stem_energy_ratios: [0.25; 4],
            spectral_centroid_hz: 1000.0,
        }
    }
}
