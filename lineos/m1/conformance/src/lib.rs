//! conformance — η διαδρομή της παράδοσης: αποκωδικοποίηση,
//! μέτρηση, διόρθωση, κωδικοποίηση, δήλωση. Καλεί τον
//! ντετερμινιστικό πυρήνα, δεν τον αντικαθιστά. Μηδέν HTTP,
//! μηδέν βάση, μηδέν διακομιστής — κατάσταση έρχεται ως όρισμα.
//!
//! symphonia lives here, and sp314-dsp still never imports it.

pub mod decode;
pub mod declare;
pub mod export;
pub mod io_flac;

// Engine files moved from m0-daemon 21/09 (F-137/PRD §6.1: run_deliver_core
// and execute_streaming_plan relocated out of m0d). m0-daemon's originals
// are now thin bridges (`pub use conformance::x::*;`) or, where a name is
// shared with a staying axum/db-adjacent caller, explicit full-path calls.
pub mod audio_source;
pub mod beat_detector;
pub mod blob_paths;
pub mod certificate_node;
pub mod content_type;
pub mod deliver;
pub mod dsp_node;
pub mod dsp_pipeline_helpers;
pub mod executor;
pub mod lazy_reader;
pub mod maestro;
pub mod mastering_progress;
pub mod nmf_worker;
pub mod scout_node;
pub mod signal_health;
pub mod spool;
pub mod standardized_decoder;
pub mod standardized_stream;
pub mod stream_core;
pub mod timeline;
pub mod input_lufs;
pub mod wav_to_raw;
