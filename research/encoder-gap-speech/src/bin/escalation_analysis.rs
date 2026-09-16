//! RECON 2026-09-16: γιατί κλιμακώνεται καθαρή αφήγηση σε stem
//! separation. Γενίκευση του secretgarden_boundaries.rs σε
//! οποιοδήποτε αρχείο· ΙΔΙΟ μονοπάτι (pass0_decode_to_dump +
//! sp314_orchestrator::trunk_pass::run_trunk_pass) — ΜΗΔΕΝ αλλαγή
//! στο boundary detection.
//!
//! Η κατωφλική λογική εδώ είναι ΑΝΤΙΓΡΑΦΟ, επαληθευμένο γραμμή-προς-
//! γραμμή έναντι lineos-corpus/src/scout.rs:239-248
//! (needs_stem_escalation) — ΙΔΙΟ AND: in_dead_zone && low_confidence.
//! Χρειάζεται αντίγραφο (όχι κλήση) μόνο για τη σάρωση ευαισθησίας,
//! όπου τα κατώφλια είναι παράμετροι· scout.rs τα έχει ως consts.
//!
//! ΧΡΗΣΗ:
//!   cargo run --release --bin escalation_analysis -- <path.mp3> [--list]

const LIBRIVOX_DIR: &str = "/home/aidevcon/Downloads/DATASET/librivox-hq";

// ΑΥΤΟΥΣΙΑ από scout.rs:235-237
const BASE_ZONE_LOW: f32 = 0.3;
const BASE_ZONE_HIGH: f32 = 0.7;
const BASE_MIN_CONF: f32 = 0.4;

fn flagged(leaning: f32, conf: f32, zone_low: f32, zone_high: f32, min_conf: f32) -> (bool, bool, bool) {
    let in_dead_zone = leaning > zone_low && leaning < zone_high;
    let low_confidence = conf < min_conf;
    (in_dead_zone, low_confidence, in_dead_zone && low_confidence)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg1 = args.get(1).cloned().unwrap_or_else(|| "secretgarden".to_string());
    let path = if arg1.ends_with(".mp3") {
        arg1.clone()
    } else {
        format!("{LIBRIVOX_DIR}/{arg1}_01_burnett.mp3")
    };
    let show_list = args.iter().any(|a| a == "--list");

    let dump = "/tmp/escalation_analysis.raw";
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(&path), dump)
        .unwrap_or_else(|e| panic!("pass0 decode {path}: {e}"));
    let trunk_report =
        sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(dump), false)
            .expect("run_trunk_pass");
    let _ = std::fs::remove_file(dump);
    let boundaries = trunk_report.boundaries;

    println!("=== {path} ===");
    println!("ΣΥΝΟΛΟ ΤΜΗΜΑΤΩΝ: {}", boundaries.len());

    if show_list {
        for (i, b) in boundaries.iter().enumerate() {
            println!(
                "{i:>3}  {:>7.2}s - {:>7.2}s  ({:>5.2}s)  {:?}  leaning={:.4} conf={:.4}",
                b.start_sec, b.end_sec, b.end_sec - b.start_sec, b.segment_type, b.avg_leaning, b.avg_confidence
            );
        }
    }

    // κατανομή κριτηρίων στα βασικά κατώφλια (0.3/0.7/0.4)
    let mut zone_only = 0usize;
    let mut conf_only = 0usize;
    let mut both = 0usize;
    let mut neither = 0usize;
    for b in &boundaries {
        let (z, c, f) = flagged(b.avg_leaning, b.avg_confidence, BASE_ZONE_LOW, BASE_ZONE_HIGH, BASE_MIN_CONF);
        match (z, c, f) {
            (_, _, true) => both += 1,
            (true, false, false) => zone_only += 1,
            (false, true, false) => conf_only += 1,
            _ => neither += 1,
        }
    }
    println!(
        "\nΚΡΙΤΗΡΙΑ (βασικά κατώφλια 0.3/0.7/0.4): flagged(AND)={both}  μόνο-zone={zone_only}  μόνο-conf={conf_only}  κανένα={neither}"
    );

    // σάρωση ευαισθησίας: conf ∈ {0.2,0.3,0.4} × ζώνη ±0.05 {(0.25,0.75),(0.30,0.70),(0.35,0.65)}
    println!("\nΣΑΡΩΣΗ ΕΥΑΙΣΘΗΣΙΑΣ (πλήθος flagged / {}):", boundaries.len());
    let zones = [(0.25f32, 0.75f32, "±0.05 πλατύ"), (0.30, 0.70, "βασικό"), (0.35, 0.65, "±0.05 στενό")];
    let confs = [0.2f32, 0.3, 0.4];
    print!("{:<14}", "zone \\ conf");
    for c in confs {
        print!("{c:>8.1}");
    }
    println!();
    for (zl, zh, label) in zones {
        print!("{:<14}", format!("{zl:.2}-{zh:.2} {label}"));
        for c in confs {
            let n = boundaries
                .iter()
                .filter(|b| flagged(b.avg_leaning, b.avg_confidence, zl, zh, c).2)
                .count();
            print!("{n:>8}");
        }
        println!();
    }
}
