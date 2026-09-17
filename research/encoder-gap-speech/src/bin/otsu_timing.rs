//! MEASURE: κόστος χρόνου της προσωρινής αλλαγής OTSU-TEMP-20260917
//! (spectral_flux.rs) πάνω στο ΠΡΑΓΜΑΤΙΚΟ run_trunk_pass — τρία
//! τρεξίματα στο dracula dump, ίδιο σχήμα με flatness_rule_timing.rs/
//! vad_organ_timing.rs. Τρέχει ΔΥΟ φορές, μία σε κάθε κατάσταση
//! (πριν/μετά το patch) — build διαφορετικό, ίδιο tool.
//! ΧΡΗΣΗ: cargo run --release --bin otsu_timing -- <dump_path> <state>
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dump_path = args.get(1).cloned().unwrap_or_else(|| "/tmp/otsu_pair_dracula.raw".to_string());
    let state = args.get(2).cloned().unwrap_or_else(|| "?".to_string());

    println!("dump={dump_path} state={state}");
    let mut times = Vec::new();
    for run in 0..3 {
        let t0 = Instant::now();
        let report = sp314_orchestrator::trunk_pass::run_trunk_pass(
            std::path::Path::new(&dump_path),
            false,
        )
        .unwrap_or_else(|e| panic!("run_trunk_pass: {e}"));
        let elapsed = t0.elapsed().as_secs_f64();
        times.push(elapsed);
        println!("  run {}: {:.3}s ({} boundaries)", run + 1, elapsed, report.boundaries.len());
    }
    let avg = times.iter().sum::<f64>() / 3.0;
    println!("state={state} μέσος_όρος_τριών_τρεξιμάτων_s={avg:.3}");
}
