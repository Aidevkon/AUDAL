//! Content-type classification — a first, honest guess at whether
//! uploaded audio is Music or Speech (podcast/audiobook — those two
//! are NOT distinguished here; see module docs below for why),
//! using only the cheap PreAnalysis pass (no NMF/Scout — that stays
//! reserved for the actual mastering pipeline).
//!
//! PROVISIONAL: thresholds below are DSP-intuition estimates, not
//! calibrated against labeled real-world data (none exists in this
//! repo yet — confirmed by recon 2026-07-17: zero speech/music
//! example fixtures in pre_analysis_contract.rs). Treat this as a
//! first flight, not a finished classifier. Do not tighten further
//! without measuring against real files first (dietary law).
//!
//! Speech vs Music, not Podcast vs Audiobook: both podcast episodes
//! and audiobook chapters look nearly identical on these cheap
//! signals (long-form clean speech). No duration threshold between
//! them is defensible without labeled data. That finer distinction
//! is deferred to a future clarifying-question flow (ask the user),
//! not solved here.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContentTypeGuess {
    Music,
    Speech,
    Uncertain,
}

#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub guess: ContentTypeGuess,
    pub confidence: f32, // 0.0-1.0
    /// The raw signal values that drove the guess, kept for
    /// transparency/debugging — never hidden reasoning.
    pub signals: GuessSignals,
}

#[derive(Debug, Clone, Copy)]
pub struct GuessSignals {
    pub crest_factor_db: f32,
    pub loudness_range: f32,
    pub sub_bass_energy_db: f32, // spectral_profile_db[0], the Sub band
    pub mid_energy_db: f32,      // spectral_profile_db[4], Mid-High band
    pub duration_secs: f64,
    pub file_count: usize,
}
