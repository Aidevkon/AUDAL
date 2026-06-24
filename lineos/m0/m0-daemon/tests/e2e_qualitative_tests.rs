//! e2e_qualitative_tests.rs
//! Qualitative DSP Assertions for Creator OS.
//! Authority: aether-black-spec-v1_0.md

use std::f32;

/// Βοηθητική συνάρτηση για υπολογισμό RMS (Root Mean Square)
fn calculate_rms(samples: &[f32]) -> f32 {
    let sum_squares: f32 = samples.iter().map(|s| s * s).sum();
    (sum_squares / samples.len() as f32).sqrt()
}

/// Το Απόλυτο Null Test: Ακυρώνει τη φάση και μετράει το "Σκουπίδι" (Residual)
/// INV-QA-3: Το σύστημα δεν εισάγει sample-drift ή broadband phase distortion.
pub fn assert_phase_coherence_null(
    original_samples: &[f32],
    mastered_samples: &[f32],
    expected_processing_gain_db: f32,
) {
    assert_eq!(
        original_samples.len(),
        mastered_samples.len(),
        "FATAL: Το Mastering άλλαξε το μήκος του αρχείου (Sample Count Mismatch)!"
    );

    // Μετατροπή του Gain από dB σε γραμμικό πολλαπλασιαστή
    // π.χ. Αν το master είναι +3dB, πρέπει να χαμηλώσουμε το mastered σήμα για να κάνουμε match.
    let gain_compensation = 10.0_f32.powf(-expected_processing_gain_db / 20.0);

    let mut residual_samples = Vec::with_capacity(original_samples.len());

    for i in 0..original_samples.len() {
        // 1. Gain Match στο Mastered
        let matched_master = mastered_samples[i] * gain_compensation;

        // 2. Phase Inversion στο Original (Πολλαπλασιασμός με -1.0)
        let inverted_original = original_samples[i] * -1.0;

        // 3. Summing (Το Nulling process)
        let residual = matched_master + inverted_original;
        residual_samples.push(residual);
    }

    let residual_rms = calculate_rms(&residual_samples);

    // Μετατροπή του Residual RMS σε dBFS (Decibels relative to Full Scale)
    let residual_dbfs = 20.0 * residual_rms.log10();

    println!("Null Test Residual: {:.2} dBFS", residual_dbfs);

    // Το όριο της αλήθειας:
    // Επειδή εφαρμόσαμε EQ και Saturation, το residual ΔΕΝ θα είναι -∞ dB (τέλεια σιωπή).
    // Όμως, αν η φάση έχει καταστραφεί ή έχουμε sample drift, το residual θα χτυπήσει -10dB με -5dB.
    // Ένα υγιές residual (μόνο EQ/Comp διαφορές) πρέπει να κάθεται κάτω από τα -30 dBFS.
    assert!(
        residual_dbfs < -30.0,
        "PHASE COHERENCE FAILURE: Το Residual είναι πολύ δυνατό ({:.2} dB). Ο κινητήρας δημιουργεί phase artifacts ή sample drift!",
        residual_dbfs
    );
}

/// Βοηθητική: Υπολογισμός Peak (Absolute Maximum)
fn calculate_peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |max, &s| max.max(s.abs()))
}

/// Βοηθητική: Υπολογισμός Crest Factor (Peak to RMS ratio) σε dB
fn calculate_crest_factor_db(samples: &[f32]) -> f32 {
    let rms = calculate_rms(samples).max(1e-9); // Αποφυγή διαίρεσης με το μηδέν
    let peak = calculate_peak(samples).max(1e-9);
    20.0 * (peak / rms).log10()
}

/// Test 1: Προστασία των Transients (Crest Factor Survival)
/// INV-QA-1: Το mastering δεν πρέπει να "λύνει" την μπότα ή να ισοπεδώνει τα δυναμικά.
pub fn assert_crest_factor_survival(
    original_samples: &[f32],
    mastered_samples: &[f32],
    max_allowed_loss_db: f32, // Το όριο συμπίεσης, π.χ. 3.0 dB
) {
    let orig_cf = calculate_crest_factor_db(original_samples);
    let mast_cf = calculate_crest_factor_db(mastered_samples);

    let cf_loss = orig_cf - mast_cf;

    println!("Original Crest Factor: {:.2} dB", orig_cf);
    println!("Mastered Crest Factor: {:.2} dB", mast_cf);
    println!("Dynamics Loss:         {:.2} dB", cf_loss);

    assert!(
        cf_loss <= max_allowed_loss_db,
        "DYNAMICS CRUSHED: Το DSP ισοπέδωσε το κομμάτι! Έχασε {:.2}dB δυναμικών (Επιτρεπτό Όριο: {}dB).",
        cf_loss, max_allowed_loss_db
    );
}

/// Test 2: Προστασία από Harshness (Spectral Tilt)
/// INV-QA-2: Το σύστημα δεν πρέπει να κάνει τα πιατίνια και τα φωνητικά να "ξυρίζουν".
/// Σημείωση: Δέχεται την ενέργεια των συχνοτήτων (από το FFT της προανάλυσης μας)
pub fn assert_spectral_tilt_bounds(
    original_mids_db: f32,  // π.χ. ενέργεια 250Hz - 4kHz
    original_highs_db: f32, // π.χ. ενέργεια 4kHz - 20kHz
    mastered_mids_db: f32,
    mastered_highs_db: f32,
    max_high_boost_db: f32, // Πόσο αέρα επιτρέπουμε, π.χ. +2.5 dB
) {
    // Ποια ήταν η ισορροπία πρίμων/μεσαίων πριν;
    let orig_tilt = original_highs_db - original_mids_db;
    // Ποια είναι η ισορροπία μετά το mastering;
    let mast_tilt = mastered_highs_db - mastered_mids_db;

    let added_harshness = mast_tilt - orig_tilt;

    println!("Added High-Frequency Tilt: {:.2} dB", added_harshness);

    assert!(
        added_harshness <= max_high_boost_db,
        "HARSHNESS DETECTED: Το EQ έδωσε πολύ αέρα/πρίμα (+{:.2}dB). Θα ματώσουν αυτιά στο TikTok!",
        added_harshness
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfect_null_with_zero_processing() {
        // Φτιάχνουμε ένα fake ημιτονοειδές κύμα (Sine wave)
        let original: Vec<f32> = (0..44100)
            .map(|i| (i as f32 * 440.0 * 2.0 * f32::consts::PI / 44100.0).sin())
            .collect();
        let mastered = original.clone(); // Καθόλου επεξεργασία

        // Αν δεν έγινε καμία επεξεργασία (0.0 dB gain), το Null Test πρέπει να βγάλει απόλυτη σιωπή (-Άπειρο dB)
        // Το test θα περάσει πανηγυρικά γιατί -Άπειρο < -30.0
        assert_phase_coherence_null(&original, &mastered, 0.0);
    }

    #[test]
    fn test_crest_factor_no_loss_with_zero_processing() {
        let original: Vec<f32> = (0..44100)
            .map(|i| (i as f32 * 440.0 * 2.0 * f32::consts::PI / 44100.0).sin())
            .collect();
        let mastered = original.clone();
        // Zero processing → zero dynamics loss
        assert_crest_factor_survival(&original, &mastered, 3.0);
    }

    #[test]
    fn test_spectral_tilt_no_harshness_with_zero_processing() {
        // Same mids/highs → zero added harshness
        assert_spectral_tilt_bounds(
            -20.0, -30.0, // original: mids, highs
            -20.0, -30.0, // mastered: same
            2.5,   // max allowed boost
        );
    }
}
