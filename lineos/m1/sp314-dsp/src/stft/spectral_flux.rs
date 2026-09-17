use crate::stft::{StftEngine, N_BINS};

pub const FLUX_THRESHOLD: f32 = 0.01_f32;
pub const FLUX_MIN_DISTANCE: usize = 10; // frames (~100ms)

pub struct SpectralFluxDetector {
    engine: StftEngine,
    pub threshold: f32,
}

impl Default for SpectralFluxDetector {
    fn default() -> Self {
        Self::new(0.01_f32)
    }
}

impl SpectralFluxDetector {
    pub fn new(threshold: f32) -> Self {
        Self {
            engine: StftEngine::new(),
            threshold,
        }
    }

    /// Compute spectral flux and detect beats.
    /// Returns: (flux_normalized: Vec<f32>,
    ///           beats: Vec<usize>)
    /// beats contains frame indices of detected onsets.
    pub fn detect(&mut self, signal: &[f32]) -> (Vec<f32>, Vec<usize>) {
        // Forward STFT
        let (frames, n_frames) = self.engine.forward(signal);

        // Compute flux per frame
        let mut flux = vec![0.0_f32; n_frames];
        let mut prev = vec![0.0_f32; N_BINS];

        for (t, frame) in frames.iter().enumerate() {
            let mut frame_flux = 0.0_f32;
            for b in 0..N_BINS {
                let mag = libm::sqrtf(frame[b].re * frame[b].re + frame[b].im * frame[b].im);
                let diff = mag - prev[b];
                if diff > 0.0_f32 {
                    frame_flux += diff;
                }
                prev[b] = mag;
            }
            flux[t] = frame_flux;
        }

        // Normalize flux to [0, 1]
        let flux_max = flux.iter().cloned().fold(0.0_f32, f32::max);

        let mut flux_norm = vec![0.0_f32; n_frames];
        if flux_max > 1e-8_f32 {
            for t in 0..n_frames {
                flux_norm[t] = flux[t] / flux_max;
            }
        }

        // Peak picking — exact same logic as Python fixture:
        // local max > threshold with min_distance
        let mut beats = Vec::new();

        // ΗΤΑΝ: `flux_norm[t] >= self.threshold`, σταθερό 0.20 (SegmentScout::new,
        // scout.rs) — έφυγε 2026-09-17. Το self.threshold μένει ως πεδίο (ο
        // constructor δεν αλλάζει, scout.rs εξακολουθεί να καλεί
        // SpectralFluxDetector::new(0.20)) αλλά δεν διαβάζεται πια εδώ — το
        // κατώφλι το δίνει η ίδια η κατανομή του παραθύρου, όχι μια σταθερά.
        //
        // ΓΙΑΤΙ: MEASURED 2026-09-17, μέσω του ΠΡΑΓΜΑΤΙΚΟΥ run_trunk_pass και
        // του /master/streaming (button path) — [ref: docs/lab-logs/
        // otsu-onset-render-pair-20260917.txt, otsu-fixtures-trunkpass-20260917.txt]:
        //   secretgarden  75→63 segments · 37→31 resets(M→S)
        //   dracula      114→84          · 56→41
        //   tale_of_two_cities: pcm_blake3 ΤΑΥΤΟΣΗΜΟ πριν/μετά (byte-για-byte)
        //   και τα επτά ήσυχα βιβλία: ΚΑΝΕΝΑ δεν ανεβάζει segments.
        //   Στα δώδεκα δοκίμια με γνωστή απάντηση: ΜΗΔΕΝ αληθινό boundary
        //   χαμένο σε κανένα από τα πέντε με εναλλαγές (5,7,8,9,10)· δύο
        //   διορθωμένα (δοκίμιο 7: 3/4→4/4, δοκίμιο 8: 4/5→5/5).
        //   Κόστος: −1.2% χρόνος (μέσα στον θόρυβο τρεξίματος) — ΜΗΔΕΝ νέο
        //   FFT, ένα ιστόγραμμα 200 κάδων ανά παράθυρο. INV-DET-1: PASS, δύο
        //   renders ίδιο SHA.
        //   HEARD: 2026-09-17, dracula @ 2058s (το ζεύγος του μεγαλύτερου
        //   max_sample_diff=0.634 ανάμεσα σε όλα τα αλλαγμένα boundaries) —
        //   ΤΟ ΠΡΙΝ έχει ακουστό zipper εκεί, το Otsu ΟΧΙ. Το 0.634 ήταν η
        //   αφαίρεση του ελαττώματος, όχι η εισαγωγή του — κόστος του F-107
        //   (κάθε ψευδές boundary μηδενίζει την αλυσίδα αποκατάστασης, και
        //   το reset ακούγεται).
        //
        // ⚠ ΔΗΛΩΜΕΝΟ ΟΡΙΟ ΤΟΥ ΟΡΓΑΝΟΥ: το Otsu βρίσκει τομή ΑΚΟΜΑ ΚΑΙ σε
        //   μονότυπο υλικό — δεν ξέρει να πει «δεν υπάρχει δεύτερος
        //   πληθυσμός». MEASURED: δοκίμιο 4 (αφήγηση με bed στα −12dB
        //   παντού, μία κλάση σε όλη τη διάρκεια) — ψευδή boundaries 4→9.
        //   Και το δοκίμιο 10 (bed συνεχίζει κάτω από αφήγηση) ακόμα χάνει
        //   ένα από τα δύο αληθινά boundaries — βελτίωση (2 χαμένα → 1),
        //   όχι λύση.
        //
        // ΠΛΑΓΙΟ: αυτή η αλλαγή αγγίζει ΚΑΙ τον δεύτερο καλούντα του
        //   SpectralFluxDetector::detect — scout_scanner.rs:60 (scan_file),
        //   που τροφοδοτεί το NMFD στα 30s (two_pass.rs, μέσα σε
        //   `if run_nmfd { ... }`). MEASURED 2026-09-17: η διαδρομή του
        //   κουμπιού (executor.rs::execute_streaming_plan → scout_node::run)
        //   περνάει use_nmfd=false ΣΤΑΘΕΡΟ — άρα εκείνος ο κλάδος δεν τρέχει
        //   σε streaming render. Νεκρό για το κουμπί, ζωντανό για την ορφανή
        //   MasterRequest/dsp_pipeline::run_dsp — ΔΕΝ μετρήθηκε εκεί.
        let otsu_thr = otsu_flux_threshold(&flux_norm);
        // Use signed arithmetic for min_distance check
        for t in 1..n_frames.saturating_sub(1) {
            if flux_norm[t] >= otsu_thr
                && flux_norm[t] > flux_norm[t - 1]
                && flux_norm[t] > flux_norm[t + 1]
            {
                let dist = if beats.is_empty() {
                    FLUX_MIN_DISTANCE + 1
                } else {
                    t - *beats.last().unwrap()
                };
                if dist >= FLUX_MIN_DISTANCE {
                    beats.push(t);
                }
            }
        }

        (flux_norm, beats)
    }
}

/// Otsu στην κατανομή του flux_norm μέσα στο παράθυρο — η τομή που
/// μεγιστοποιεί τη διακύμανση ανάμεσα στις δύο κλάσεις (πλαίσια με
/// επίθεση / χωρίς). Ίδια μέθοδος με trunk_pass.rs's otsu_split_bin
/// (N. Otsu, "A Threshold Selection Method from Gray-Level Histograms",
/// IEEE Trans. SMC, vol. 9, 1979, DOI 10.1109/TSMC.1979.4310076),
/// εφαρμοσμένη εδώ σε ιστόγραμμα 200 κάδων του ήδη κανονικοποιημένου
/// [0,1] flux αντί για ένα προϋπολογισμένο ιστόγραμμα RMS.
fn otsu_flux_threshold(flux_norm: &[f32]) -> f32 {
    const NBINS: usize = 200;
    const BIN_WIDTH: f32 = 1.0 / NBINS as f32;
    let mut hist = [0u32; NBINS];
    for &v in flux_norm {
        let b = ((v / BIN_WIDTH) as usize).min(NBINS - 1);
        hist[b] += 1;
    }
    let total: f64 = hist.iter().map(|&c| c as f64).sum();
    if total == 0.0 {
        return 0.5;
    }
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut w0, mut sum0) = (0.0_f64, 0.0_f64);
    let (mut best_var, mut best_t) = (-1.0_f64, 0usize);
    for t in 0..NBINS {
        w0 += hist[t] as f64;
        if w0 == 0.0 {
            continue;
        }
        let w1 = total - w0;
        if w1 == 0.0 {
            break;
        }
        sum0 += t as f64 * hist[t] as f64;
        let m0 = sum0 / w0;
        let m1 = (sum_all - sum0) / w1;
        let var = w0 * w1 * (m0 - m1) * (m0 - m1);
        if var > best_var {
            best_var = var;
            best_t = t;
        }
    }
    if best_var < 0.0 {
        0.5
    } else {
        (best_t as f32 + 0.5) * BIN_WIDTH
    }
}
