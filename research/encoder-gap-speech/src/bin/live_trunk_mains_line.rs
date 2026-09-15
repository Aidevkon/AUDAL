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

    // ── ΤΑ ΕΝΝΙΑ ΜΕ ΤΟ ΟΡΓΑΝΟ ΤΗΣ ΠΑΡΑΓΩΓΗΣ, 2026-09-15 ──
    // ΜΗΔΕΝ ζητούμενο εδώ: αυτά τα αρχεία δεν είχαν ποτέ γνωστή/ζητούμενη
    // συχνότητα (σε αντίθεση με τα odyssey/swiss/mobydick παραπάνω, όπου
    // ζητούμενο σήμαινε "τι ζητήσαμε να ενεθεί"). Εδώ μετράμε ΤΙ ΒΡΙΣΚΕΙ
    // ο ζωντανός ανιχνευτής στην ΕΙΣΟΔΟ, τίποτα άλλο. Ίδια μέθοδος
    // decode-to-dump → run_trunk_pass — ΜΗΔΕΝ επεξεργασία στο μεσοδιάστημα.
    // Το run_trunk_pass καλείται στο ΙΔΙΟ αρχείο dump που ήδη
    // χρησιμοποιεί το run_one παραπάνω — η είσοδος, πριν από ΟΤΙΔΗΠΟΤΕ.
    println!("\n── ΤΑ ΕΙΚΟΣΙ (9 WAKING + 11 SLEEPING, interior-floor-20260912.txt) ──");
    println!("(WAKING = interior >= -60dB στο παλιό όργανο· SLEEPING = interior < -60dB. Ουδεμία σχέση με το mains-hum όργανο — μόνο ταυτοποίηση αρχείου.)\n");

    let base = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    let waking = [
        "adventurespinocchio_01_collodi",
        "anne_of_green_gables_01_montgomery",
        "count_of_monte_cristo_001_dumas",
        "dracula_01_stoker",
        "huckfinn_01_twain_apc",
        "janeeyre_01_bronte",
        "peterpan_01_barrie",
        "secretgarden_01_burnett",
        "tale_of_two_cities_01_dickens",
    ];
    let sleeping = [
        "adventureholmes_01_doyle",
        "adventuresholmes_01_doyle",
        "emma_01_01_austen",
        "mobydick_000_melville",
        "odyssey_01_homer_butler",
        "prideandprejudice_10-11_austen",
        "returnofholmes_01_doyle",
        "robinson_crusoe_01_defoe",
        "swiss_family_robinson_01_wyss",
        "treasure_island_01-02_stevenson",
        "uncletom_01_stowe",
    ];

    let mut results: Vec<(String, Option<f32>, Option<f32>, Option<f32>)> = Vec::new();

    println!("-- WAKING (9) --");
    for name in waking {
        let (hz, prom, pos) = run_measure(base, name);
        results.push((name.to_string(), hz, prom, pos));
    }
    println!("\n-- SLEEPING (11) --");
    for name in sleeping {
        let (hz, prom, pos) = run_measure(base, name);
        results.push((name.to_string(), hz, prom, pos));
    }

    // ── ΒΗΜΑ 2: αποσπάσματα ακρόασης, ΩΜΑ, γύρω από την ΙΔΙΑ παύση ──
    // ΜΗΔΕΝ κρίση εδώ: εξάγονται ΟΛΑ όσα έδωσαν θέση, ταξινομημένα κατά
    // προεξοχή φθίνουσα. Η ακρόαση αποφασίζει, όχι αυτό το εργαλείο.
    println!("\n── ΒΗΜΑ 2: αποσπάσματα 10s (5s πριν την παύση + η παύση) ──");
    let out_dir = "/tmp/hum-audition";
    let _ = std::fs::create_dir_all(out_dir);

    let mut with_pos: Vec<&(String, Option<f32>, Option<f32>, Option<f32>)> = results
        .iter()
        .filter(|(_, _, prom, pos)| prom.is_some() && pos.is_some())
        .collect();
    with_pos.sort_by(|a, b| b.2.unwrap().partial_cmp(&a.2.unwrap()).unwrap());

    for (name, _hz, prom, pos) in &with_pos {
        let pos = pos.unwrap();
        let prom = prom.unwrap();
        let clip_start = (pos - 5.0).max(0.0);
        let out_path = format!(
            "{out_dir}/{name}_prom{:.2}dB_at{:.1}s.wav",
            prom, pos
        );
        let path = format!("{base}/{name}.mp3");
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                &format!("{clip_start:.3}"),
                "-i",
                &path,
                "-t",
                "10",
                "-acodec",
                "pcm_s16le",
                &out_path,
            ])
            .status();
        match status {
            Ok(s) if s.success() => println!("  γράφτηκε: {out_path}"),
            _ => println!("  ⚠ ffmpeg ΑΠΕΤΥΧΕ για {name}"),
        }
    }

    println!("\nΑπόσπασμα ΧΩΡΙΣ θέση παύσης (mains_line ή θέση None): {}",
        results.iter().filter(|(_, _, prom, pos)| prom.is_none() || pos.is_none()).count());
    for (name, hz, prom, pos) in &results {
        if prom.is_none() || pos.is_none() {
            println!("    {name}: hz={hz:?} prom={prom:?} pos={pos:?}");
        }
    }
}

const SR: u32 = 48_000;

/// ΜΕΤΡΑ ΚΑΙ ΑΝΑΦΕΡΕΙ — μηδέν ζητούμενο, μηδέν κατώφλι, μηδέν κρίση.
/// Επιστρέφει (hz, prominence_db, θέση_παύσης_sec) ώστε ο main να μπορεί
/// να εξάγει αποσπάσματα ακρόασης γύρω από την ΙΔΙΑ παύση που μέτρησε
/// ο ανιχνευτής — ΟΧΙ ξαναϋπολογισμένη.
///
/// ΣΗΜΕΙΩΣΗ ΘΕΣΗΣ: το `TrunkReport.mains_line` δεν εκθέτει τη θέση της
/// παύσης (μόνο hz/prominence). Η θέση ανακτάται εδώ καλώντας ΤΙΣ ΙΔΙΕΣ
/// δημόσιες συναρτήσεις του sp314-dsp (`otsu_pause_threshold_db`,
/// `longest_run_below` — μία πηγή, §0.2 του detector task) πάνω στο ΙΔΙΟ
/// mono buffer, με το `quiet_window_split_dbfs` που ΗΔΗ επέστρεψε το
/// run_trunk_pass ως κατώφλι — ΔΕΝ ξαναϋπολογίζεται το Otsu, μόνο η θέση.
fn run_measure(base: &str, name: &str) -> (Option<f32>, Option<f32>, Option<f32>) {
    let path = format!("{base}/{name}.mp3");
    let dump = format!("/tmp/live_trunk_{name}.raw");
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(&path), &dump)
        .expect("pass0 decode");
    let report = sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(&dump), false)
        .expect("run_trunk_pass");

    let bytes = std::fs::read(&dump).expect("read dump");
    let _ = std::fs::remove_file(&dump);
    let n = bytes.len() / 4;
    let mut inter = Vec::with_capacity(n);
    for i in 0..n {
        inter.push(f32::from_le_bytes([
            bytes[i * 4],
            bytes[i * 4 + 1],
            bytes[i * 4 + 2],
            bytes[i * 4 + 3],
        ]));
    }
    let mono: Vec<f32> = inter.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect();

    let pause_start_s = report.quiet_window_split_dbfs.and_then(|thr| {
        sp314_dsp::analysis::mains_hum::longest_run_below(&mono, SR, thr, None)
            .map(|(start, _len)| start as f32 / SR as f32)
    });

    match report.mains_line {
        Some(line) => {
            println!(
                "  {name:<40} Hz={:.3} prominence_db={:.3} (decim1k, ΝΕΑ ΜΕΤΡΗΣΗ) quiet_split={:?} παύση@{:.1}s",
                line.hz, line.prominence_db, report.quiet_window_split_dbfs,
                pause_start_s.unwrap_or(-1.0)
            );
            (Some(line.hz), Some(line.prominence_db), pause_start_s)
        }
        None => {
            println!(
                "  {name:<40} None (quiet_split={:?}) — δεν μετρήθηκε",
                report.quiet_window_split_dbfs
            );
            (None, None, None)
        }
    }
}
