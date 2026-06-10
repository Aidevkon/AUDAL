// analysis/features.rs — re-exports from lineos-types
// StemFeatures moved to lineos-types for layer isolation (S-007)
pub use lineos_types::analysis::{
    MixMetrics, StemFeatures, StemMetrics, ANALYSIS_FFT_SIZE, ANALYSIS_HOP_SIZE,
    ANALYSIS_SAMPLE_RATE, DYNAMIC_RANGE_BLOCK_MS, ENERGY_RATIO_EPSILON, LRA_BLOCK_MS, LRA_HOP_MS,
};
