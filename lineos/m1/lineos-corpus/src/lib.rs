pub mod builder;
pub mod contract;
pub mod features;

pub use features::StateFeatures;

pub mod reader;
pub use reader::{extract_features_sequence, extract_state_sequence, read_corpus_sessions};

pub mod model;
pub use model::{EmissionHistogram, TransitionMatrix};

pub mod inference;
pub use inference::{predict_state, StatePrediction, StemMarkovModel};

pub mod store;
pub use store::{aggregate_all_presets, aggregate_preset, PresetMarkovModel, UserMarkovModel};

pub mod mfcc;
pub use mfcc::{MelFilterbank, MfccAnalyzer};

pub mod genre_centroid;
pub use genre_centroid::{
    compute_bucket_centroid, compute_global_stats, format_f32_const, gate, CentroidError,
    GateCriteria, RejectReason, TrackGateInputs, GATE_V1_ACOUSTIC, GATE_V1_IDM, GATE_V1_TECHNO,
};

pub mod classifier;
pub mod genre_centroids_generated;
pub use classifier::{GenreClassifier, MAX_DISTANCE_THRESHOLD, MIN_DISTANCE_DELTA};
pub mod scout;
