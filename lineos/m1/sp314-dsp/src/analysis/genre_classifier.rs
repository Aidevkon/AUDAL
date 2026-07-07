//! genre_classifier.rs
//! Deterministic, Z-scored MFCC classification for Creator OS.
//! Authority: MEA-001 (Measurement-First)

pub const GLOBAL_MFCC_MEAN: [f32; 13] = [
    -11.282939, 4.619359, -0.058864, 0.501321, -0.170580, 0.042619, -0.102155, -0.096856,
    -0.013764, -0.008137, -0.049515, -0.088642, -0.183755,
];

pub const GLOBAL_MFCC_STD: [f32; 13] = [
    5.141149, 2.860325, 1.630640, 1.107524, 0.978694, 0.814911, 0.702522, 0.689514, 0.601996,
    0.570613, 0.520369, 0.525024, 0.494410,
];

pub const IDM_MFCC_MEAN: [f32; 13] = [
    -0.028942, 0.050562, 0.335884, 0.066955, 0.350688, 0.257811, 0.187719, 0.268231, 0.186681,
    0.190227, 0.149608, 0.127562, 0.153356,
];

pub const ACOUSTIC_MFCC_MEAN: [f32; 13] = [
    0.036802, -0.064271, -0.427232, -0.085166, -0.446066, -0.327918, -0.238772, -0.341179,
    -0.237452, -0.241960, -0.190298, -0.162255, -0.195061,
];

// thresholds determined through intra/inter class variance recon
pub const MAX_DISTANCE_THRESHOLD: f32 = 4.0;
pub const MIN_DISTANCE_DELTA: f32 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genre {
    Idm,
    Acoustic,
}

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
        // /tmp/genre_references/idm/coolkid - Synchronicity.mp3
        let raw_idm_track = [
            -16.073845, 7.182110, 0.391454, 1.006915, 0.252865, 0.241503, 0.055008, -0.002895,
            -0.192724, -0.136454, -0.155973, -0.103782, -0.179701,
        ];

        let result = GenreClassifier::classify(&raw_idm_track);
        assert_eq!(result, Some(Genre::Idm));
    }
}
