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
    expected_processing_gain_db: f32
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfect_null_with_zero_processing() {
        // Φτιάχνουμε ένα fake ημιτονοειδές κύμα (Sine wave)
        let original: Vec<f32> = (0..44100).map(|i| (i as f32 * 440.0 * 2.0 * f32::consts::PI / 44100.0).sin()).collect();
        let mastered = original.clone(); // Καθόλου επεξεργασία

        // Αν δεν έγινε καμία επεξεργασία (0.0 dB gain), το Null Test πρέπει να βγάλει απόλυτη σιωπή (-Άπειρο dB)
        // Το test θα περάσει πανηγυρικά γιατί -Άπειρο < -30.0
        assert_phase_coherence_null(&original, &mastered, 0.0);
    }
}
