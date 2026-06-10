// analysis/mod.rs — S-002 Stem Feature Analyzer
// Authority: spec/locked/S-002_stem_feature_analyzer.md v1.1

pub mod analyzer;
pub mod dynamics;
pub mod features;
pub mod pre_analysis;
pub mod sdr;
pub mod spectral;
pub mod stereo;

pub use analyzer::StemFeatureAnalyzer;
pub use features::{MixMetrics, StemFeatures, StemMetrics};
pub use pre_analysis::PreAnalyzer;
pub mod phantom_master;
pub use phantom_master::PhantomMaster;
