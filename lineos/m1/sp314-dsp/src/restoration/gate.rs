// src/restoration/gate.rs
// Noise Gate — downward expander.
// Constitutional: libm only, zero allocation, precomputed coefficients.

use libm::{expf, fabsf};

/// Noise Gate — downward expander to silence background noise.
/// Features a linked stereo detector, hold time to prevent chatter,
/// and smooth exponential attack/release curves.
pub struct NoiseGate {
    threshold_linear: f32, // -45 dBFS default
    floor_linear: f32,     // precomputed: EXPANDER_FLOOR_DB
    attack_coef: f32,      // precomputed: 1ms
    release_coef: f32,     // precomputed: 100ms
    hold_samples: usize,   // precomputed: 50ms hold before closing
    hold_counter: usize,
    gain: f32, // current gate gain [floor_linear, 1.0]
    enabled: bool,
}

// SOURCE: πρότυπο βιομηχανίας για φωνή,
// RETRIEVED: 2026-09-11 — downward expander, όχι
// gate. Ratio εκτός απείρου δίνει κλίση αντί για
// γκρεμό, και floor αντί για σιωπή.
// ΗΤΑΝ 0.0 (σκληρό κλείσιμο): μετρήθηκε 06/09 σε
// πραγματική αφήγηση ότι σβήνει το room tone —
// πάτωμα εξόδου −180,62 dBFS — που η ACX ΑΠΑΙΤΕΙ
// (1-5 s στα άκρα). Και η ακρόαση της διαφοράς,
// πάνω από 1 kHz και +20 dB, ήταν καθαρή ομιλία.
const EXPANDER_RATIO: f32 = 1.5;

// SOURCE: πρότυπο βιομηχανίας για φωνή,
// RETRIEVED: 2026-09-11 — το range/floor ορίζει
// πόση εξασθένηση εφαρμόζεται όταν ο κόμβος είναι
// κλειστός· η ACX θέλει μείωση 6-8 dB τη φορά, όχι
// ένα επιθετικό πέρασμα.
// floor −12 dB: απόφαση ιδιοκτήτη 2026-09-11.
// ΔΗΛΩΜΕΝΟ ΟΡΙΟ: σταθερό floor. Όταν το κατώφλι
// γίνει δυναμικό (F-096), το floor το ακολουθεί —
// δύο άγνωστοι μαζί δεν κρίνονται.
const EXPANDER_FLOOR_DB: f32 = -12.0;

impl NoiseGate {
    // gate_threshold_db is the absolute target threshold. The default (-45 dBFS)
    // now lives in the orchestrator; the core only receives an absolute threshold.
    // pad_db_shift remains as orthogonal headroom compensation.
    pub fn new(sample_rate: f32, pad_db_shift: f32, gate_threshold_db: f32) -> Self {
        Self {
            threshold_linear: libm::powf(10.0, (gate_threshold_db + pad_db_shift) / 20.0),
            floor_linear: libm::powf(10.0, EXPANDER_FLOOR_DB / 20.0),
            attack_coef: expf(-1.0 / (sample_rate * 0.001)), // 1ms open
            release_coef: expf(-1.0 / (sample_rate * 0.100)), // 100ms close
            hold_samples: (sample_rate * 0.050) as usize,    // 50ms hold
            hold_counter: 0,
            gain: 1.0,
            enabled: true,
        }
    }

    #[inline]
    fn compute_gain(&mut self, level: f32) -> f32 {
        let target_gain = if level >= self.threshold_linear {
            self.hold_counter = self.hold_samples; // reset hold
            1.0_f32
        } else if self.hold_counter > 0 {
            self.hold_counter -= 1;
            1.0_f32 // still open during hold
        } else {
            // Downward expander: για κάθε dB που το σήμα πέφτει κάτω από
            // το κατώφλι, η εξασθένηση αυξάνεται κατά (ratio − 1) dB, και
            // ΔΕΝ ξεπερνάει ποτέ το floor.
            //   target = max(floor_lin, 10^(−(ratio−1)·Δ/20)),  Δ = κατώφλι − στάθμη
            // Ισοδύναμο χωρίς λογάριθμο: (level / threshold)^(ratio − 1).
            // Στη σιωπή (level = 0) το powf δίνει 0 και το floor δαγκώνει —
            // ποτέ ψηφιακό μηδέν.
            libm::fmaxf(
                self.floor_linear,
                libm::powf(level / self.threshold_linear, EXPANDER_RATIO - 1.0),
            )
        };

        // Smooth gain changes — attack when opening, release when closing
        let coef = if target_gain > self.gain {
            self.attack_coef
        } else {
            self.release_coef
        };

        self.gain = self.gain + (target_gain - self.gain) * (1.0 - coef);
        self.gain
    }

    #[inline]
    pub fn process_stereo(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled {
            return (left, right);
        }

        // Use max of L/R for detection (linked stereo gate)
        let level = fabsf(left).max(fabsf(right));
        let gain = self.compute_gain(level);

        (left * gain, right * gain)
    }

    #[inline]
    pub fn process_mono(&mut self, sample: f32) -> f32 {
        if !self.enabled {
            return sample;
        }

        let level = fabsf(sample);
        let gain = self.compute_gain(level);

        sample * gain
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
        self.hold_counter = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::PI;

    const SR: f32 = 48_000.0;
    const THR_DB: f32 = -45.0;

    fn db_to_lin(db: f32) -> f32 {
        libm::powf(10.0, db / 20.0)
    }

    fn lin_to_db(lin: f32) -> f32 {
        20.0 * libm::log10f(lin)
    }

    /// Οδηγεί τον κόμβο με ημίτονο 1 kHz, κορυφής `peak_db`, για 2 s —
    /// πολύ πάνω από το hold (50 ms) και το release (100 ms) — και
    /// επιστρέφει το ΜΕΓΙΣΤΟ gain των τελευταίων 10 ms.
    ///
    /// Μέγιστο και όχι τελευταίο δείγμα: το gain ταλαντώνεται μέσα στην
    /// περίοδο του ημιτόνου, άρα η τιμή στο τελευταίο δείγμα εξαρτάται
    /// από τη φάση. Η κορυφή είναι η σταθερή κατάσταση που ακούγεται.
    fn settled_gain_for_sine(peak_db: f32) -> f32 {
        let mut g = NoiseGate::new(SR, 0.0, THR_DB);
        let amp = db_to_lin(peak_db);
        let n = (SR * 2.0) as usize;
        let window_start = n - (SR * 0.010) as usize;

        let mut max_gain = 0.0_f32;
        for i in 0..n {
            let s = amp * libm::sinf(2.0 * PI * 1000.0 * i as f32 / SR);
            let _ = g.process_mono(s);
            if i >= window_start {
                max_gain = libm::fmaxf(max_gain, g.gain);
            }
        }
        max_gain
    }

    #[test]
    fn sine_at_threshold_passes_untouched() {
        // Test A — στο κατώφλι ο κόμβος δεν αγγίζει τίποτα.
        let gain = settled_gain_for_sine(THR_DB);
        let att_db = lin_to_db(gain);
        assert!(
            att_db.abs() < 0.1,
            "στο κατώφλι το gain πρέπει να είναι 1.0· μετρήθηκε {gain} ({att_db} dB)"
        );
    }

    #[test]
    fn sine_six_db_below_is_attenuated_by_half_as_much() {
        // Test B — ratio 1.5 ⇒ κλίση (1.5 − 1) = 0.5 ⇒ 6 dB κάτω δίνει
        // 3 dB εξασθένηση. ΑΥΤΟ είναι ο expander: κλίση, όχι γκρεμός.
        let gain = settled_gain_for_sine(THR_DB - 6.0);
        let att_db = lin_to_db(gain);
        assert!(
            (att_db + 3.0).abs() < 0.2,
            "6 dB κάτω ⇒ ≈ −3 dB· μετρήθηκε {att_db} dB"
        );
    }

    #[test]
    fn floor_bites_before_the_slope_runs_away() {
        // Test C — ΤΟ ΚΡΙΣΙΜΟ. 40 dB κάτω, η κλίση θα έδινε −20 dB.
        // Το floor κόβει στα −12. Αν αυτό το τεστ δει −20, το floor
        // δεν δαγκώνει και ο κόμβος ξανάγινε γκρεμός.
        let gain = settled_gain_for_sine(THR_DB - 40.0);
        let att_db = lin_to_db(gain);
        assert!(
            (att_db - EXPANDER_FLOOR_DB).abs() < 0.1,
            "40 dB κάτω ⇒ ΑΚΡΙΒΩΣ {EXPANDER_FLOOR_DB} dB (όχι −20)· μετρήθηκε {att_db} dB"
        );
    }

    #[test]
    fn silence_never_becomes_digital_zero() {
        // Test D — το room tone της ACX. Η σιωπή βγαίνει εξασθενημένη
        // κατά το floor, ΠΟΤΕ μηδενισμένη: το παλιό 0.0 έσβηνε τα
        // 1-5 s που η ACX απαιτεί στα άκρα.
        let mut g = NoiseGate::new(SR, 0.0, THR_DB);
        let n = (SR * 2.0) as usize;
        for _ in 0..n {
            let _ = g.process_mono(0.0);
        }
        assert!(
            g.gain > 0.0,
            "η σιωπή δεν επιτρέπεται να δώσει ψηφιακό μηδέν· gain = {}",
            g.gain
        );
        let att_db = lin_to_db(g.gain);
        assert!(
            (att_db - EXPANDER_FLOOR_DB).abs() < 0.1,
            "η σιωπή κάθεται στο floor {EXPANDER_FLOOR_DB} dB· μετρήθηκε {att_db} dB"
        );
    }
}
