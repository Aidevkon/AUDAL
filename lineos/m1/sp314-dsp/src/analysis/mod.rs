// analysis/mod.rs — S-002 Stem Feature Analyzer
// Authority: spec/locked/S-002_stem_feature_analyzer.md v1.1

pub mod acx_check;
pub mod analyzer;
pub mod dynamics;
pub mod features;
pub mod mel_128;
pub mod onset_flux;
pub mod pre_analysis;
pub mod sdr;
pub mod spectral;
pub mod stereo;
pub mod superflux_weights;

pub use analyzer::StemFeatureAnalyzer;
pub use features::{MixMetrics, StemFeatures, StemMetrics};
pub use pre_analysis::{
    loudness_range_lu, spectral_profile_levels, spectral_slope, Biquad, PreAnalyzer, BAND_EDGES,
};
pub mod phantom_master;
pub use phantom_master::PhantomMaster;
pub mod ear_fatigue;
pub use ear_fatigue::{EarFatigueDelta, EarFatigueModel};
pub mod morph_curve;
pub use morph_curve::{CurveType, MorphCurve};
pub mod album_conductor;
pub use album_conductor::{AlbumConductor, TrackPlan};
pub use lineos_types::pre_analysis::Genre;
pub mod scout;
pub mod scout_scanner;
pub mod stem_escalation;
pub mod vad_features;
pub mod vad_model;
pub mod vad_sensors;
pub mod role;
pub mod routing;
pub mod phi1_sensor;
