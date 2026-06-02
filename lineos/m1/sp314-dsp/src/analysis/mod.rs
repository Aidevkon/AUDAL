// analysis/mod.rs — S-002 Stem Feature Analyzer
// Authority: spec/locked/S-002_stem_feature_analyzer.md v1.1

pub mod features;
pub mod spectral;
pub mod dynamics;
pub mod stereo;
pub mod analyzer;
pub mod pre_analysis;
pub mod sdr;

pub use features::{StemFeatures, StemMetrics, MixMetrics};
pub use analyzer::StemFeatureAnalyzer;
pub use pre_analysis::PreAnalyzer;
