//! RECON 2026-09-18 βοηθητικό: ΕΝΑ τρέξιμο του ΠΡΑΓΜΑΤΙΚΟΥ
//! run_trunk_pass πάνω σε έναν ήδη-αποκωδικοποιημένο dump — σχεδιασμένο
//! να τρέχει κάτω από `/usr/bin/time -v` (μία διεργασία ανά τρέξιμο,
//! ώστε η κορυφή μνήμης να είναι καθαρή ανά τρέξιμο, όχι σωρευτική).
//! ΧΡΗΣΗ: /usr/bin/time -v ./run_trunk_once <dump.raw>
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dump = &args[1];
    let t0 = Instant::now();
    let report = sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(dump), false)
        .unwrap_or_else(|e| panic!("run_trunk_pass: {e}"));
    let elapsed = t0.elapsed().as_secs_f64();
    println!("elapsed_s={elapsed:.3} boundaries={}", report.boundaries.len());
}
