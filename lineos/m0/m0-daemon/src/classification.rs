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

/// Three independent heuristics vote; each contributes evidence
/// toward Speech or Music. Unanimous or near-unanimous votes give
/// high confidence; split votes give Uncertain — the honest
/// admission that cheap signals alone can't decide, and a
/// clarifying question is the right next step (not a 4th
/// heuristic to force a decision).
pub fn guess_content_type(
    m: &sp314_orchestrator::trunk_pass::TrunkMetrics,
    duration_secs: f64,
    file_count: usize,
) -> ClassificationResult {
    let sub_bass_energy_db = m.spectral_profile_db[0];
    let mid_energy_db = m.spectral_profile_db[4];
    let signals = GuessSignals {
        crest_factor_db: m.crest_db,
        loudness_range: m.lra,
        sub_bass_energy_db,
        mid_energy_db,
        duration_secs,
        file_count,
    };

    // Vote 1 — crest factor: speech is peaky (loud words, silent
    // gaps); mastered music is dense/limited. PROVISIONAL split at
    // 14 dB — typical mastered music sits ~6-10 dB, conversational
    // speech commonly 15-20+ dB.
    let crest_votes_speech = signals.crest_factor_db > 14.0;

    // Vote 2 — sub-bass presence: music mixes almost always carry
    // deliberate sub/bass energy; close-mic'd speech recordings
    // are typically high-pass filtered or naturally lack sub
    // content. PROVISIONAL: sub more than 18 dB quieter than the
    // vocal-presence mid band suggests no deliberate low end.
    let bass_votes_speech = (mid_energy_db - sub_bass_energy_db) > 18.0;

    // Vote 3 — loudness range: speech (unprocessed, dynamic
    // delivery) tends to have a wider LRA than a compressed/
    // limited music master. PROVISIONAL split at 12 LU.
    let lra_votes_speech = signals.loudness_range > 12.0;

    let speech_votes = [crest_votes_speech, bass_votes_speech, lra_votes_speech]
        .iter()
        .filter(|&&v| v)
        .count();

    let (guess, confidence) = match speech_votes {
        3 => (ContentTypeGuess::Speech, 0.85),
        0 => (ContentTypeGuess::Music, 0.85),
        2 => (ContentTypeGuess::Speech, 0.55),
        1 => (ContentTypeGuess::Music, 0.55),
        _ => unreachable!("speech_votes is a count of 3 booleans, 0..=3"),
    };
    // Below a minimum confidence, be honest rather than confident:
    let (guess, confidence) = if confidence < 0.6 {
        (ContentTypeGuess::Uncertain, confidence)
    } else {
        (guess, confidence)
    };

    ClassificationResult {
        guess,
        confidence,
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_trunk_metrics(
        crest_db: f32,
        lra: f32,
        sub_db: f32,
        mid_db: f32,
    ) -> sp314_orchestrator::trunk_pass::TrunkMetrics {
        sp314_orchestrator::trunk_pass::TrunkMetrics {
            integrated_lufs: Some(-16.0),
            // Not read by guess_content_type; placeholder like the other
            // fixed fields below. Added because TrunkMetrics gained the
            // field (rms_db was previously discarded in trunk_pass.rs).
            rms_db: -18.0,
            crest_db,
            lra,
            noise_floor_dbfs: Some(-60.0),
            spectral_profile_db: [sub_db, -20.0, -18.0, -15.0, mid_db, -18.0, -22.0, -30.0],
            transient_density: 0.5,
            global_phase_correlation: 1.0,
            dynamic_range_db: 10.0,
        }
    }

    #[test]
    fn clean_speech_profile_guesses_speech_with_high_confidence() {
        // High crest (peaky), wide LRA, near-zero sub-bass relative to mids.
        let m = synthetic_trunk_metrics(18.0, 15.0, -40.0, -12.0);
        let result = guess_content_type(&m, 1800.0, 1);
        assert_eq!(result.guess, ContentTypeGuess::Speech);
        assert!(
            result.confidence >= 0.8,
            "expected high confidence, got {}",
            result.confidence
        );
    }

    #[test]
    fn dense_mastered_music_profile_guesses_music_with_high_confidence() {
        // Low crest (limited/dense), tight LRA, present sub-bass.
        let m = synthetic_trunk_metrics(7.0, 5.0, -8.0, -10.0);
        let result = guess_content_type(&m, 210.0, 1);
        assert_eq!(result.guess, ContentTypeGuess::Music);
        assert!(
            result.confidence >= 0.8,
            "expected high confidence, got {}",
            result.confidence
        );
    }

    #[test]
    fn ambiguous_profile_yields_uncertain_not_a_forced_guess() {
        // Deliberately split evidence: high crest (speech-like) but
        // present sub-bass and tight LRA (music-like) — e.g. a lo-fi
        // beat with a vocal sample, or spoken word over a music bed.
        let m = synthetic_trunk_metrics(16.0, 6.0, -9.0, -10.0);
        let result = guess_content_type(&m, 240.0, 1);
        assert_eq!(result.guess, ContentTypeGuess::Uncertain);
    }

    #[test]
    fn signals_are_preserved_for_transparency() {
        let m = synthetic_trunk_metrics(18.0, 15.0, -40.0, -12.0);
        let result = guess_content_type(&m, 1800.0, 1);
        assert_eq!(result.signals.crest_factor_db, 18.0);
        assert_eq!(result.signals.duration_secs, 1800.0);
        assert_eq!(result.signals.file_count, 1);
    }
}
