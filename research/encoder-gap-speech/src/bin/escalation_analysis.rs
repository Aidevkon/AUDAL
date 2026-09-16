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
//!   cargo run --release --bin escalation_analysis -- <path.mp3> --merge-sweep
//!
//! MEASURE 2026-09-16 (F-107 followup): --merge-sweep προσθέτει ένα
//! βήμα συγχώνευσης ΜΕΤΑ τον τεμαχιστή — ΜΟΝΟ εδώ, στο εργαλείο.
//! ΜΗΔΕΝ αλλαγή στον scout ή στον τεμαχιστή παραγωγής. Πολιτική
//! συγχώνευσης, δηλωμένη: τμήμα μικρότερο από Χ ενώνεται με τον
//! ΜΕΓΑΛΥΤΕΡΟ (σε διάρκεια) από τους δύο γείτονές του — όχι πάντα τον
//! προηγούμενο (θα ήταν αυθαίρετη χρονική προτίμηση χωρίς λόγο σε ένα
//! offline εργαλείο ανάλυσης, που βλέπει όλο το αρχείο εκ των
//! προτέρων) και όχι "ίδιου τύπου" (δεν υπάρχει τέτοιος γείτονας — ο
//! τεμαχιστής εναλλάσσει πάντα τύπο, δομικά). Λογική: το μεγαλύτερο
//! κομμάτι έχει περισσότερη ένδειξη υπέρ της ταξινόμησής του — αν το
//! μικρό τμήμα είναι τρέμουλο, το πιο πιθανό είναι να ανήκει στο
//! μεγαλύτερο γειτονικό σήμα, όχι στο μικρότερο. Μετά από κάθε
//! συγχώνευση, δύο πλέον-γειτονικά τμήματα ΙΔΙΟΥ τύπου ενώνονται
//! αυτόματα (δομική αναγκαιότητα — δύο διαδοχικά Speech δεν είναι
//! δύο τμήματα). avg_leaning/avg_confidence των συγχωνεύσεων:
//! duration-weighted μέσος όρος των ΗΔΗ υπολογισμένων per-segment
//! μέσων όρων — ΟΧΙ επανυπολογισμός από τα ωμά per-window δεδομένα
//! (δεν διατηρούνται εδώ).

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

#[derive(Clone, Debug)]
struct MSeg {
    start: f32,
    end: f32,
    ty: lineos_corpus::scout::SegmentType,
    leaning: f32,
    conf: f32,
    music_sec: f32,
    speech_sec: f32,
}

fn seg_dur(s: &MSeg) -> f32 {
    s.end - s.start
}

fn to_mseg(b: &lineos_corpus::scout::SegmentBoundary) -> MSeg {
    use lineos_corpus::scout::SegmentType;
    let dur = b.end_sec - b.start_sec;
    let (music_sec, speech_sec) = match b.segment_type {
        SegmentType::Music => (dur, 0.0),
        SegmentType::Speech => (0.0, dur),
    };
    MSeg {
        start: b.start_sec,
        end: b.end_sec,
        ty: b.segment_type,
        leaning: b.avg_leaning,
        conf: b.avg_confidence,
        music_sec,
        speech_sec,
    }
}

fn combine(a: &MSeg, b: &MSeg, winner_type: lineos_corpus::scout::SegmentType) -> MSeg {
    let da = seg_dur(a);
    let db = seg_dur(b);
    let total = (da + db).max(1e-6);
    MSeg {
        start: a.start.min(b.start),
        end: a.end.max(b.end),
        ty: winner_type,
        leaning: (a.leaning * da + b.leaning * db) / total,
        conf: (a.conf * da + b.conf * db) / total,
        music_sec: a.music_sec + b.music_sec,
        speech_sec: a.speech_sec + b.speech_sec,
    }
}

/// Δομική αναγκαιότητα, ΟΧΙ πολιτική μικρού-μήκους: δύο πλέον-γειτονικά
/// τμήματα ίδιου τύπου (συνέπεια προηγούμενης συγχώνευσης) ενώνονται.
fn coalesce_adjacent(mut segs: Vec<MSeg>) -> Vec<MSeg> {
    loop {
        let mut merged_any = false;
        let mut out: Vec<MSeg> = Vec::with_capacity(segs.len());
        let mut i = 0;
        while i < segs.len() {
            if i + 1 < segs.len() && segs[i].ty == segs[i + 1].ty {
                out.push(combine(&segs[i], &segs[i + 1], segs[i].ty));
                i += 2;
                merged_any = true;
            } else {
                out.push(segs[i].clone());
                i += 1;
            }
        }
        segs = out;
        if !merged_any {
            break;
        }
    }
    segs
}

fn merge_short(boundaries: &[lineos_corpus::scout::SegmentBoundary], min_len: f32) -> Vec<MSeg> {
    let mut segs: Vec<MSeg> = boundaries.iter().map(to_mseg).collect();
    if min_len <= 0.0 {
        return segs; // Χ=0: σημερινή συμπεριφορά, καμία συγχώνευση
    }
    loop {
        if segs.len() <= 1 {
            break;
        }
        let (idx, shortest_dur) = segs
            .iter()
            .enumerate()
            .map(|(i, s)| (i, seg_dur(s)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        if shortest_dur >= min_len {
            break;
        }
        // γείτονας: ο ΜΕΓΑΛΥΤΕΡΟΣ από τους δύο· στα άκρα, ο μόνος διαθέσιμος.
        let neighbor_idx = if idx == 0 {
            1
        } else if idx == segs.len() - 1 {
            idx - 1
        } else if seg_dur(&segs[idx + 1]) > seg_dur(&segs[idx - 1]) {
            idx + 1
        } else {
            idx - 1
        };
        let winner_type = segs[neighbor_idx].ty;
        let (lo, hi) = if idx < neighbor_idx { (idx, neighbor_idx) } else { (neighbor_idx, idx) };
        let merged = combine(&segs[lo], &segs[hi], winner_type);
        segs.remove(hi);
        segs.remove(lo);
        segs.insert(lo, merged);
        segs = coalesce_adjacent(segs);
    }
    segs
}

struct SweepRow {
    x: f32,
    segments: usize,
    music_to_speech_boundaries: usize,
    flagged: usize,
    music_became_speech_sec: f32,
    speech_became_music_sec: f32,
}

fn measure_at(boundaries: &[lineos_corpus::scout::SegmentBoundary], x: f32) -> SweepRow {
    use lineos_corpus::scout::SegmentType;
    let segs = merge_short(boundaries, x);
    let segments = segs.len();
    let music_to_speech_boundaries = segs
        .windows(2)
        .filter(|w| w[0].ty == SegmentType::Music && w[1].ty == SegmentType::Speech)
        .count();
    let flagged = segs
        .iter()
        .filter(|s| flagged(s.leaning, s.conf, BASE_ZONE_LOW, BASE_ZONE_HIGH, BASE_MIN_CONF).2)
        .count();
    let music_became_speech_sec: f32 = segs.iter().filter(|s| s.ty == SegmentType::Speech).map(|s| s.music_sec).sum();
    let speech_became_music_sec: f32 = segs.iter().filter(|s| s.ty == SegmentType::Music).map(|s| s.speech_sec).sum();
    SweepRow {
        x,
        segments,
        music_to_speech_boundaries,
        flagged,
        music_became_speech_sec,
        speech_became_music_sec,
    }
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
    let merge_sweep = args.iter().any(|a| a == "--merge-sweep");

    let dump = "/tmp/escalation_analysis.raw";
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(&path), dump)
        .unwrap_or_else(|e| panic!("pass0 decode {path}: {e}"));
    let trunk_report =
        sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(dump), false)
            .expect("run_trunk_pass");
    let _ = std::fs::remove_file(dump);
    let boundaries = trunk_report.boundaries;

    if merge_sweep {
        // ΣΥΜΠΛΗΡΩΜΑ 2026-09-16: WINDOW_SECS/HOP_SECS επαληθευμένα στο
        // trunk_pass.rs:187-189 ("Mirror scan_file's constants exactly
        // [scout_scanner.rs:6-7]") — 5.0/1.0, ΙΔΙΑ PROVISIONAL σφραγίδα
        // με το smooth_and_segment (scout_scanner.rs:4-5: "validated via
        // Flights 9-12... but"), όχι θεωρητική επιλογή του εργαλείου.
        const WINDOW_SECS: f32 = 5.0;
        const HOP_SECS: f32 = 1.0;
        let below_window = boundaries.iter().filter(|b| b.end_sec - b.start_sec < WINDOW_SECS).count();
        println!("=== {path} ===");
        println!(
            "ΤΜΗΜΑΤΑ < ΠΑΡΑΘΥΡΟ ({WINDOW_SECS}s) ΣΗΜΕΡΑ (Χ=0): {below_window} / {} ({:.1}%)",
            boundaries.len(),
            100.0 * below_window as f32 / boundaries.len() as f32
        );
        println!(
            "{:>6}  {:>9}  {:>10}  {:>8}  {:>18}  {:>18}",
            "X(s)", "τμήματα", "M→S ορια", "flagged", "Music→Speech(s)", "Speech→Music(s)"
        );
        for x in [0.0f32, 2.0, 3.0, WINDOW_SECS - HOP_SECS, WINDOW_SECS, WINDOW_SECS + HOP_SECS, 10.0] {
            let row = measure_at(&boundaries, x);
            let tag = if x == WINDOW_SECS {
                " ← ΠΑΡΑΘΥΡΟ ΑΝΑΛΥΣΗΣ"
            } else if x == WINDOW_SECS - HOP_SECS {
                " ← παράγωγο: ΠΑΡΑΘΥΡΟ−hop"
            } else if x == WINDOW_SECS + HOP_SECS {
                " ← παράγωγο: ΠΑΡΑΘΥΡΟ+hop"
            } else {
                ""
            };
            println!(
                "{:>6.0}  {:>9}  {:>10}  {:>8}  {:>18.2}  {:>18.2}{tag}",
                row.x, row.segments, row.music_to_speech_boundaries, row.flagged, row.music_became_speech_sec, row.speech_became_music_sec
            );
        }
        return;
    }

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
