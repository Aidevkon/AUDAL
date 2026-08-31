// analysis/clipping.rs — clip EVENT counting for the certificate.
//
// F-085 wiring (2026-08-25): το `StoredQuality.clips_detected` ήταν
// σταθερά `0` από γεννησιμιού του πεδίου, επειδή **κανένας clip
// counter δεν υπήρχε στο δέντρο**. Αυτό είναι ο counter.

/// Απόλυτο κατώφλι δείγματος. 0.999 και όχι 1.0: μετά από
/// κβαντισμό/resample μια κομμένη κορυφή σπάνια κάθεται ακριβώς στο
/// 1.0, αλλά δεν κατεβαίνει ουσιαστικά κάτω από αυτό.
pub const CLIP_THRESHOLD: f32 = 0.999;

/// Ελάχιστο μήκος ριπής για να μετρήσει ως γεγονός.
/// ΔΥΟ δεν αρκούν: ένα ημίτονο δειγματοληπτημένο κοντά στην κορυφή
/// του μπορεί να δώσει δύο γειτονικά δείγματα πάνω από το κατώφλι
/// χωρίς να έχει κοπεί τίποτα.
pub const CLIP_MIN_RUN: usize = 3;

/// Μετρητής γεγονότων clipping, incremental (streaming-safe).
///
/// ΟΡΙΣΜΟΣ — ΡΗΤΟΣ, γιατί χωρίς αυτόν το «3 clips» δεν σημαίνει
/// τίποτα:
/// · clip event = **ΤΡΙΑ Ή ΠΕΡΙΣΣΟΤΕΡΑ ΔΙΑΔΟΧΙΚΑ** δείγματα με
///   `|x| >= CLIP_THRESHOLD`, στο **ΙΔΙΟ** κανάλι
/// · μετράμε **ΓΕΓΟΝΟΤΑ**, όχι δείγματα: μια ριπή 847 δειγμάτων
///   είναι **ΕΝΑ** γεγονός
/// · **ένα μεμονωμένο δείγμα στο 1.0 ΔΕΝ είναι clip** — είναι κορυφή
///   που ακουμπάει το ταβάνι
/// · κανάλια: **ΑΘΡΟΙΣΜΑ** — γεγονός σε κάθε κανάλι μετράει χωριστά
///
/// ⚠ ΜΕΤΡΑΕΙ ΤΗΝ ΕΞΟΔΟ — δικό μας clipping.
/// ΔΕΝ ΑΝΙΧΝΕΥΕΙ clipping ΤΗΣ ΕΙΣΟΔΟΥ: μετά από gain/EQ/LTASS οι
/// επίπεδες κορυφές μετακινούνται και δεν κάθονται πια στο ταβάνι. Η
/// ζημιά μένει, ο ανιχνευτής δεν τη βλέπει. 0 εδώ ΔΕΝ σημαίνει «η
/// ηχογράφηση είναι καθαρή».
/// Input clipping = ΝΕΟ πεδίο στο `input_delivery_*` μπλοκ, δικό του
/// βήμα (πύλη εισόδου).
#[derive(Debug, Default, Clone)]
pub struct ClipEventCounter {
    run_l: usize,
    run_r: usize,
    events: u32,
}

impl ClipEventCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Τροφοδοτεί ένα κομμάτι. Η ριπή συνεχίζεται πέρα από τα όρια
    /// του chunk — γι' αυτό το μήκος ζει στο struct, όχι στη μέθοδο.
    pub fn feed_chunk(&mut self, left: &[f32], right: &[f32]) {
        let n = left.len().min(right.len());
        for i in 0..n {
            Self::step(&mut self.run_l, &mut self.events, left[i]);
            Self::step(&mut self.run_r, &mut self.events, right[i]);
        }
    }

    /// Ένα δείγμα, ένα κανάλι. Το γεγονός μετριέται στη στιγμή που η
    /// ριπή ΦΤΑΝΕΙ το ελάχιστο μήκος — όχι όταν τελειώνει. Έτσι μια
    /// ριπή που φτάνει ως το τέλος του αρχείου μετριέται κι αυτή, και
    /// το `finish()` δεν χρειάζεται να κλείσει τίποτα.
    #[inline]
    fn step(run: &mut usize, events: &mut u32, sample: f32) {
        if sample.abs() >= CLIP_THRESHOLD {
            *run += 1;
            if *run == CLIP_MIN_RUN {
                *events += 1;
            }
        } else {
            *run = 0;
        }
    }

    pub fn finish(self) -> u32 {
        self.events
    }
}

/// Batch εκδοχή για καλούντες που κρατούν ήδη ολόκληρα τα κανάλια
/// (offline certificate path). Ταυτόσημος ορισμός με τον streaming —
/// ίδιο struct από κάτω, ώστε να μην μπορούν να αποκλίνουν.
pub fn count_clip_events(left: &[f32], right: &[f32]) -> u32 {
    let mut c = ClipEventCounter::new();
    c.feed_chunk(left, right);
    c.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(peak: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| peak * libm::sinf(2.0 * core::f32::consts::PI * 440.0 * (i as f32) / 48_000.0))
            .collect()
    }

    /// ORACLE 1 — καθαρό ημίτονο −6 dBFS: κανένα γεγονός.
    #[test]
    fn f085_clean_sine_has_no_clips() {
        let s = sine(0.501, 48_000); // ≈ −6 dBFS
        assert_eq!(count_clip_events(&s, &s), 0);
    }

    /// ORACLE 2 — ΤΟ ΚΡΙΣΙΜΟ: κορυφή ΑΚΡΙΒΩΣ στο 1.0 σε μεμονωμένα
    /// σημεία ΔΕΝ είναι clip. Αυτό ξεχωρίζει «ακουμπάει το ταβάνι»
    /// από «κόπηκε».
    #[test]
    fn f085_isolated_full_scale_peaks_are_not_clips() {
        let mut s = sine(0.5, 48_000);
        for i in [100usize, 5_000, 20_000, 44_100] {
            s[i] = 1.0; // ΕΝΑ δείγμα, όχι ριπή
        }
        assert_eq!(count_clip_events(&s, &s), 0);
    }

    /// ORACLE 3 — ΑΚΡΙΒΩΣ 5 ριπές × 10 δείγματα ⇒ 5 γεγονότα, ΟΧΙ 50.
    #[test]
    fn f085_counts_events_not_samples() {
        let mut s = sine(0.5, 48_000);
        for burst in 0..5 {
            let start = 1_000 + burst * 5_000;
            for k in 0..10 {
                s[start + k] = 1.0;
            }
        }
        let silent = vec![0.0f32; s.len()];
        assert_eq!(count_clip_events(&s, &silent), 5);
    }

    /// ORACLE 4 — μόνο το ΔΕΞΙ κανάλι κομμένο, 3 ριπές ⇒ 3.
    /// Επιβεβαιώνει ότι η μέτρηση είναι ανά κανάλι και αθροίζεται.
    #[test]
    fn f085_per_channel_events_sum() {
        let clean = sine(0.5, 48_000);
        let mut right = clean.clone();
        for burst in 0..3 {
            let start = 2_000 + burst * 6_000;
            for k in 0..8 {
                right[start + k] = -1.0; // αρνητικό: ελέγχει το abs()
            }
        }
        assert_eq!(count_clip_events(&clean, &right), 3);
    }

    /// Streaming ≡ batch, ΚΑΙ όταν η ριπή πέφτει πάνω σε όριο chunk.
    #[test]
    fn f085_streaming_matches_batch_across_chunk_boundary() {
        let mut s = sine(0.5, 10_000);
        // ριπή που ΔΙΑΣΧΙΖΕΙ το όριο των 4096
        for k in 0..12 {
            s[4_090 + k] = 1.0;
        }
        let batch = count_clip_events(&s, &s);
        let mut c = ClipEventCounter::new();
        for chunk in s.chunks(4_096) {
            c.feed_chunk(chunk, chunk);
        }
        assert_eq!(batch, c.finish());
        assert_eq!(batch, 2, "μία ριπή ανά κανάλι, δύο κανάλια");
    }
}
