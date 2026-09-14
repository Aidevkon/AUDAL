//! ΕΠΑΛΗΘΕΥΣΗ: τρέχει τη ΖΩΝΤΑΝΗ διαδρομή (run_trunk_pass_with_acx, το ίδιο
//! που καλεί ο executor.rs) σε τρία αρχεία που ήδη μετρήθηκαν με Welch48k
//! (hum_spectrum, hum-spectrum-20260914 / hum-detector-recon-20260914) και
//! συγκρίνει ΜΟΝΟ τη συχνότητα — η προεξοχή ΔΕΝ συγκρίνεται, διαφορετικό
//! όργανο (βλ. analysis::mains_hum's module doc, sp314-dsp).
//!
//! ΑΝΟΧΗ, ΔΗΛΩΜΕΝΗ ΠΡΙΝ ΤΡΕΞΕΙ: ±0.5 Hz — ίδια ανοχή με το
//! mains_hum_detector.rs test (sp314-dsp), ίδιος λόγος: το detect_mains_line
//! βρίσκει δυναμικά την κορυφή, η ζωντανή αλυσίδα βρίσκει την ΠΑΥΣΗ μέσω
//! διαφορετικού μονοπάτου δεδομένων (qw_env αντί για ολόκληρο buffer) —
//! μπορεί να διαφέρει ελαφρώς ΠΟΙΑ ακριβώς παύση επιλέγεται.
//!
//! ΧΡΗΣΗ: cargo run --release --bin live_trunk_mains_line

const FREQ_TOL_HZ: f32 = 0.5;

fn run_one(path: &str, tag: &str, expect_hz: f32) {
    let dump = format!("/tmp/live_trunk_{tag}.raw");
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .expect("pass0 decode");
    let report = sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(&dump), false)
        .expect("run_trunk_pass");
    let _ = std::fs::remove_file(&dump);

    match report.mains_line {
        Some(line) => {
            let err = (line.hz - expect_hz).abs();
            let verdict = if err <= FREQ_TOL_HZ { "ΕΝΤΟΣ" } else { "⚠ ΕΚΤΟΣ" };
            println!(
                "  {tag:<10} ζητούμενο={expect_hz:.2}Hz μετρημένο(ζωντανή)={:.3}Hz prom={:.3}dB(decim1k, ΝΕΑ ΜΕΤΡΗΣΗ) Δ={err:.3}Hz {verdict}",
                line.hz, line.prominence_db
            );
        }
        None => {
            println!("  {tag:<10} ζητούμενο={expect_hz:.2}Hz μετρημένο(ζωντανή)=None (δεν μετρήθηκε — δεν βρέθηκε τομή Otsu ή παύση) ⚠ ΕΚΤΟΣ");
        }
    }
}

fn main() {
    println!("Ανοχή δηλωμένη πριν το τρέξιμο: ±{FREQ_TOL_HZ} Hz, μόνο συχνότητα.\n");
    run_one(
        "/home/aidevcon/Downloads/DATASET/librivox-hq/odyssey_01_homer_butler.mp3",
        "odyssey",
        60.01,
    );
    run_one(
        "/home/aidevcon/Downloads/DATASET/librivox-hq/swiss_family_robinson_01_wyss.mp3",
        "swiss",
        59.90,
    );
    run_one(
        "/home/aidevcon/Downloads/DATASET/librivox-hq/mobydick_000_melville.mp3",
        "mobydick",
        70.16,
    );
}
