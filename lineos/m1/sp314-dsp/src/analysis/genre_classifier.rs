//! genre_classifier.rs
//! Deterministic, Z-scored MFCC classification for Creator OS.
//! Authority: MEA-001 (Measurement-First)

use super::genre_centroids_generated::{
    ACOUSTIC_MFCC_MEAN, GLOBAL_MFCC_MEAN, GLOBAL_MFCC_STD, IDM_MFCC_MEAN,
};

// thresholds determined through intra/inter class variance recon
pub const MAX_DISTANCE_THRESHOLD: f32 = 4.0;
pub const MIN_DISTANCE_DELTA: f32 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genre {
    Idm,
    Acoustic,
}

/// MFCC centroids sourced from genre_centroids_generated.rs
/// (measure_corpus output) as of genre-corpus-v2-wiring-v3 — see that
/// file's header for corpus provenance and the IDM-provisional caveat.
///
/// Classification is MFCC-only (13-dim Z-scored Euclidean) — this is
/// v1/v2 scope (S-0XX §6). BPM/onset detection (v3, PreAnalysisData.bpm
/// — currently hardcoded 0.0 everywhere) is a SEPARATE future signal,
/// not a classifier input today. When it lands, the intended
/// integration point is a second-stage tie-breaker or gate criterion
/// downstream of classify()'s Option<Genre> result — NOT a change to
/// the 13-dim MFCC distance math itself, which stays a clean, isolated,
/// independently-testable unit. Do not preemptively add BPM parameters
/// or fields to this classifier now; that would be speculative
/// plumbing ahead of the actual v3 corpus/spec work.
pub struct GenreClassifier;

impl GenreClassifier {
    /// Accepts raw (un-normalized) MFCC mean from a track.
    /// Z-Scores it against the global corpus parameters, and computes the nearest centroid.
    pub fn classify(raw_track_mean: &[f32; 13]) -> Option<Genre> {
        let mut z_track = [0.0; 13];
        for i in 0..13 {
            z_track[i] = (raw_track_mean[i] - GLOBAL_MFCC_MEAN[i]) / (GLOBAL_MFCC_STD[i] + 1e-8);
        }

        let mut dist_idm_sq = 0.0;
        let mut dist_acoustic_sq = 0.0;
        for i in 0..13 {
            let d_i = z_track[i] - IDM_MFCC_MEAN[i];
            let d_a = z_track[i] - ACOUSTIC_MFCC_MEAN[i];
            dist_idm_sq += d_i * d_i;
            dist_acoustic_sq += d_a * d_a;
        }

        let dist_idm = dist_idm_sq.sqrt();
        let dist_acoustic = dist_acoustic_sq.sqrt();

        let (min_dist, genre) = if dist_idm < dist_acoustic {
            (dist_idm, Genre::Idm)
        } else {
            (dist_acoustic, Genre::Acoustic)
        };

        if min_dist > MAX_DISTANCE_THRESHOLD {
            return None; // Completely out of bounds (e.g. death metal or sine sweeps)
        }

        if (dist_idm - dist_acoustic).abs() < MIN_DISTANCE_DELTA {
            return None; // Ambiguous gray zone
        }

        Some(genre)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genre_classification_idm_track() {
        // Create a raw track that perfectly matches the IDM centroid in Z-space
        let mut raw_idm = [0.0; 13];
        for i in 0..13 {
            raw_idm[i] = IDM_MFCC_MEAN[i] * (GLOBAL_MFCC_STD[i] + 1e-8) + GLOBAL_MFCC_MEAN[i];
        }
        let result = GenreClassifier::classify(&raw_idm);
        assert_eq!(result, Some(Genre::Idm));
    }

    #[test]
    fn test_genre_classification_acoustic_track() {
        // Create a raw track that perfectly matches the ACOUSTIC centroid in Z-space
        let mut raw_acoustic = [0.0; 13];
        for i in 0..13 {
            raw_acoustic[i] =
                ACOUSTIC_MFCC_MEAN[i] * (GLOBAL_MFCC_STD[i] + 1e-8) + GLOBAL_MFCC_MEAN[i];
        }
        let result = GenreClassifier::classify(&raw_acoustic);
        assert_eq!(result, Some(Genre::Acoustic));
    }

    #[test]
    fn test_genre_classification_unknown_noise() {
        // Track with values way outside the normalized bounds should return None
        let raw_noise = [100.0; 13];
        let result = GenreClassifier::classify(&raw_noise);
        assert_eq!(result, None);
    }

    #[test]
    fn test_genre_classification_ambiguous() {
        // Track exactly halfway between IDM and Acoustic in Z-space
        let mut raw_middle = [0.0; 13];
        for i in 0..13 {
            let mid_z = (IDM_MFCC_MEAN[i] + ACOUSTIC_MFCC_MEAN[i]) / 2.0;
            raw_middle[i] = mid_z * (GLOBAL_MFCC_STD[i] + 1e-8) + GLOBAL_MFCC_MEAN[i];
        }
        let result = GenreClassifier::classify(&raw_middle);
        assert_eq!(result, None); // FAILS the MIN_DISTANCE_DELTA check
    }

    #[test]
    fn test_genre_classification_real_idm_track() {
        // Raw MFCC mean for an actual IDM reference track:
        // idm/BoDleasons - Our Journey.mp3
        // measured via the corrected 48kHz-resampled pipeline, genre-corpus-v2-wiring-v3 —
        // replaces a cd5890c-era fixture (coolkid - Synchronicity.mp3) that was contaminated
        // by the pre-fix MFCC pipeline AND independently fails today's LUFS gate
        // (LufsTooLow -16.4 vs -14.0), making it unsuitable as either a measurement
        // fixture or a reference-quality example.
        let raw_idm_track = [
            -8.680081,
            2.7254674,
            0.52635294,
            0.4284627,
            0.04749891,
            0.2554113,
            -0.08625301,
            0.06384678,
            0.08782445,
            0.16598931,
            0.08583771,
            -0.09737512,
            -0.16520621,
        ];

        let result = GenreClassifier::classify(&raw_idm_track);
        assert_eq!(result, Some(Genre::Idm));
    }
}
