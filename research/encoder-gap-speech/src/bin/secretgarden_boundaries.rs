//! ΜΕΤΡΗΣΗ 2026-09-16: ξαναπαράγει τα boundaries (75 τμήματα, ίδια
//! μέθοδος με streaming_pipeline_ducking_e2e/executor.rs) για το
//! secretgarden_01_burnett, για επιβεβαίωση χρόνου/ταξινόμησης πριν
//! επιλεγεί απόσπασμα ακρόασης. ΜΗΔΕΝ αλλαγή στο boundary detection.

const PATH: &str = "/home/aidevcon/Downloads/DATASET/librivox-hq/secretgarden_01_burnett.mp3";

fn main() {
    // ΙΔΙΟ μονοπάτι με το πραγματικό executor.rs:322 — trunk_report.boundaries,
    // ΟΧΙ build_timeline_map/WholeBufferProvider (12-λεπτο όριο, το βιβλίο
    // είναι 808s). Καμία διαφορά μεθόδου, μόνο αποφυγή ενός ορίου whole-buffer
    // decode που η πραγματική διαδρομή δεν έχει καθόλου (streaming dump).
    let dump = "/tmp/secretgarden_boundaries.raw";
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(PATH), dump)
        .expect("pass0 decode");
    let trunk_report =
        sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(dump), false)
            .expect("run_trunk_pass");
    let _ = std::fs::remove_file(dump);
    let boundaries = trunk_report.boundaries;
    println!("ΣΥΝΟΛΟ ΤΜΗΜΑΤΩΝ: {}", boundaries.len());
    for (i, b) in boundaries.iter().enumerate() {
        println!(
            "{i:>3}  {:>7.2}s - {:>7.2}s  ({:>5.2}s)  {:?}  leaning={:.4} conf={:.4}",
            b.start_sec,
            b.end_sec,
            b.end_sec - b.start_sec,
            b.segment_type,
            b.avg_leaning,
            b.avg_confidence
        );
    }

    // Δηλωμένη επαλήθευση έναντι FINDINGS.md/F-102: segment 5 = πρώτο flagged, Music.
    let flagged = |idx: usize| -> bool {
        let b = &boundaries[idx];
        let leaning_dead_zone = b.avg_leaning > 0.3 && b.avg_leaning < 0.7;
        leaning_dead_zone && b.avg_confidence < 0.4
    };
    println!("\nΕπαλήθευση FINDINGS.md/F-102 (leaning dead-zone 0.3-0.7 KAI confidence<0.4):");
    for i in [5usize, 7, 13, 17, 18, 22, 27, 29, 34, 36, 37, 38, 39, 47, 49, 67, 71] {
        if i < boundaries.len() {
            println!(
                "  seg {i}: flagged={} type={:?} leaning={:.4} conf={:.4} [{:.2}s-{:.2}s]",
                flagged(i),
                boundaries[i].segment_type,
                boundaries[i].avg_leaning,
                boundaries[i].avg_confidence,
                boundaries[i].start_sec,
                boundaries[i].end_sec
            );
        }
    }
}
