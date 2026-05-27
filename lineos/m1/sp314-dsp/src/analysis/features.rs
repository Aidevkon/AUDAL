// analysis/features.rs — re-exports from lineos-types
// StemFeatures moved to lineos-types for layer isolation (S-007)
pub use lineos_types::analysis::{
    StemFeatures, StemMetrics, MixMetrics,
    ANALYSIS_FFT_SIZE, ANALYSIS_HOP_SIZE,
    ANALYSIS_SAMPLE_RATE, LRA_BLOCK_MS,
    LRA_HOP_MS, DYNAMIC_RANGE_BLOCK_MS,
    ENERGY_RATIO_EPSILON,
};
