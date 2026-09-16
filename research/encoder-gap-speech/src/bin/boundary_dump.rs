//! Βοηθητικό: τυπώνει τα boundaries (start_sec, end_sec, segment_type)
//! του ΠΡΑΓΜΑΤΙΚΟΥ run_trunk_pass πάνω σε ένα dump — για σύγκριση
//! πριν/μετά την προσωρινή αλλαγή του trunk_pass.rs (TEMP_FLATNESS_
//! RULE_20260917). ΔΕΝ αγγίζει παραγωγή.
//! ΧΡΗΣΗ: cargo run --release --bin boundary_dump -- <dump_path>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dump_path = &args[1];
    let report = sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(dump_path), false)
        .unwrap_or_else(|e| panic!("run_trunk_pass failed: {e}"));
    println!("boundaries={}", report.boundaries.len());
    for b in &report.boundaries {
        println!("{:.3}\t{:.3}\t{:?}\t{:.4}\t{:.4}", b.start_sec, b.end_sec, b.segment_type, b.avg_leaning, b.avg_confidence);
    }
}
