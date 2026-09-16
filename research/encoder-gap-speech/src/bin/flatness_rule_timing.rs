//! MEASURE 2026-09-17: κόστος χρόνου του run_trunk_pass, με και χωρίς
//! τον προσωρινό ασύμμετρο κανόνα επιπεδότητας (ΒΗΜΑ 0.1). Read-only
//! ως προς αυτό το εργαλείο· η ΠΑΡΑΓΩΓΗ (trunk_pass.rs) αλλάζει
//! προσωρινά ΕΞΩ από αυτό το αρχείο, με σφραγίδα TEMP_FLATNESS_RULE_
//! 20260917, και επαναφέρεται στο ίδιο task.
//!
//! Τρέχει το ΠΡΑΓΜΑΤΙΚΟ sp314_orchestrator::trunk_pass::run_trunk_pass
//! τρεις φορές στο ΙΔΙΟ dump, μετράει wall-clock ανά κλήση. Καλείται
//! μία φορά ΠΡΙΝ την προσωρινή αλλαγή (baseline) και μία φορά ΜΕΤΑ
//! (με το patch), ίδιο αρχείο, ίδιο binary structure — μόνο ο κώδικας
//! του trunk_pass.rs διαφέρει ανάμεσα στις δύο κλήσεις αυτού του bin.
//!
//! ΧΡΗΣΗ: cargo run --release --bin flatness_rule_timing -- <dump_path>

use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dump_path = args.get(1).cloned().unwrap_or_else(|| "/tmp/timing_dracula.raw".to_string());
    let path = std::path::Path::new(&dump_path);
    if !path.exists() {
        panic!("dump not found: {dump_path} — build it first (pass0_decode_to_dump)");
    }

    println!("dump={dump_path}");
    let mut times = Vec::new();
    for i in 0..3 {
        let t0 = Instant::now();
        let report = sp314_orchestrator::trunk_pass::run_trunk_pass(path, false)
            .unwrap_or_else(|e| panic!("run_trunk_pass failed: {e}"));
        let elapsed = t0.elapsed();
        times.push(elapsed.as_secs_f64());
        println!(
            "  run {}: {:.3}s  (boundaries={}, windows={})",
            i + 1, elapsed.as_secs_f64(), report.boundaries.len(), report.metrics.cv_ioi_sequence.len()
        );
    }
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!("  μέσος όρος: {:.3}s", mean);
}
