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

    run_ltass_resolver_measure(base, &waking);

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

use lineos_types::analysis::ANALYSIS_SAMPLE_RATE as SR;

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

// ── ΜΕΤΡΗΣΗ 2026-09-16: ΤΙ ΖΗΤΑΕΙ Ο RESOLVER ΣΤΑ ΕΝΝΙΑ WAKING ──────
//
// spectral_profile_db ΤΟΥ TrunkReport = ΑΥΤΟΥΣΙΟ ό,τι φτάνει στο
// pre.spectral_profile_db της ζωντανής streaming_pipeline.rs
// (trunk_pass.rs:175, `to_pre_analysis`: «spectral_profile_db:
// self.spectral_profile_db» — μηδέν μετασχηματισμός ενδιάμεσα).
// ΕΠΙΛΟΓΗ (ζητήθηκε ρητά): καλείται ο ReferenceResolver ΑΠΕΥΘΕΙΑΣ με
// αυτό το πεδίο — ΟΧΙ execute_streaming_plan. Ίδια κλήση, ίδια
// κανονικοποίηση με streaming_pipeline.rs:369-380 (μέσος όρος των
// πρώτων `normalization_band_count` μπαντών αφαιρείται).
//
// Ο πίνακας B⁻¹ (q=0.707) και το G_MAX_DB=6.0 παρακάτω είναι ΑΝΤΙΓΡΑΦΟ,
// ΟΧΙ κλήση, της `apply_ltass_band_compensation`
// (sp314-orchestrator/src/streaming_pipeline.rs): η συνάρτηση είναι
// ιδιωτική στο crate της και ΔΕΝ αγγίχτηκε γι' αυτή τη μέτρηση («ΜΗΔΕΝ
// αλλαγή κώδικα παραγωγής»). Οι ίδιοι 64 συντελεστές ήδη επαληθεύτηκαν
// byte-for-byte έναντι του log στο προηγούμενο MEASURE βήμα (unit test
// `s1_probe_matches_the_generators_log`, sp314-orchestrator).
#[rustfmt::skip]
const B_INV_Q0707: [[f32; 8]; 8] = [
    [ 1.3717, -0.5135,  0.2316, -0.0911,  0.0310, -0.0102,  0.0030, -0.0008],
    [-0.4907,  1.7330, -1.0605,  0.4278, -0.1457,  0.0482, -0.0141,  0.0038],
    [ 0.1502, -0.7726,  2.0203, -1.1928,  0.4248, -0.1406,  0.0413, -0.0110],
    [-0.0507,  0.2677, -1.1396,  2.2964, -1.2724,  0.4424, -0.1301,  0.0347],
    [ 0.0199, -0.1049,  0.4686, -1.3901,  2.3899, -1.2671,  0.3908, -0.1044],
    [-0.0073,  0.0388, -0.1739,  0.5396, -1.3760,  2.2805, -1.0778,  0.3007],
    [ 0.0024, -0.0129,  0.0579, -0.1802,  0.4809, -1.1825,  1.9069, -0.8234],
    [-0.0006,  0.0032, -0.0142,  0.0443, -0.1185,  0.3047, -0.7204,  1.7053],
];
const G_MAX_DB: f32 = 6.0;
const LTASS_CFS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];

/// Στήλη 1: `target[k]-signal[k]`, dead-zone εφαρμοσμένο — ίδια λογική
/// με `ReferenceResolver::compute_gains` (aether-bridge/reference_resolver.rs:192-207),
/// ΜΕΙΟΝ την τελευταία γραμμή της (το `fminf(g_max, fmaxf(-g_max, raw))`
/// clamp). «Ό,τι ζητάει ο resolver πριν το ταβάνι» σημαίνει ακριβώς αυτό.
fn raw_request_unclamped(
    signal: &[f32; 8],
    target: &[f32; 8],
    dead_zone_db: &[f32; 8],
) -> [f32; 8] {
    std::array::from_fn(|k| {
        let raw = target[k] - signal[k];
        if raw.abs() <= dead_zone_db[k] {
            0.0
        } else {
            raw
        }
    })
}

fn b_inv_unclamped(raw: [f32; 8]) -> [f32; 8] {
    std::array::from_fn(|i| {
        B_INV_Q0707[i]
            .iter()
            .zip(raw.iter())
            .map(|(&b, &r)| b * r)
            .sum()
    })
}

fn clamp_to_ceiling(v: [f32; 8]) -> [f32; 8] {
    std::array::from_fn(|i| v[i].clamp(-G_MAX_DB, G_MAX_DB))
}

fn run_ltass_resolver_measure(base: &str, waking: &[&str]) {
    println!("\n══ ΤΙ ΖΗΤΑΕΙ Ο RESOLVER ΣΤΑ ΕΝΝΙΑ WAKING (δικό μας υλικό, librivox-hq) ══");
    println!("Στήλες ανά μπάντα: RAW (πριν πίνακα, πριν ταβάνι) · POST_B (μετά πίνακα, πριν ταβάνι) · FINAL (μετά ταβάνι).\n");

    let profile = aether_bridge::reference_resolver::ReferenceProfile::load(
        aether_bridge::reference_resolver::ProfileId::PodcastV1,
    );
    let n = profile.normalization_band_count;

    let mut all_raw: Vec<f32> = Vec::with_capacity(9 * 8);
    let mut ceiling_hits: Vec<(String, usize)> = Vec::new();
    let mut matrix_pushes_over: Vec<(String, usize)> = Vec::new();

    for &name in waking {
        let path = format!("{base}/{name}.mp3");
        let dump = format!("/tmp/live_trunk_ltass_{name}.raw");
        m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(&path), &dump)
            .expect("pass0 decode");
        let report =
            sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(&dump), false)
                .expect("run_trunk_pass");
        let _ = std::fs::remove_file(&dump);

        let signal_raw = report.metrics.spectral_profile_db;
        let speech_mean: f32 = signal_raw[..n].iter().sum::<f32>() / n as f32;
        let normalized: [f32; 8] = std::array::from_fn(|k| signal_raw[k] - speech_mean);

        let raw = raw_request_unclamped(&normalized, &profile.spectral_target, &profile.dead_zone_db);
        let post_b = b_inv_unclamped(raw);
        let fin = clamp_to_ceiling(post_b);

        println!("  {name}");
        println!(
            "    {:<9} {:>9} {:>9} {:>9} {:>9}",
            "band", "hz", "RAW", "POST_B", "FINAL"
        );
        for k in 0..8 {
            println!(
                "    band {k}   {:>9.1} {:>9.3} {:>9.3} {:>9.3}",
                LTASS_CFS[k], raw[k], post_b[k], fin[k]
            );
            all_raw.push(raw[k]);
            if post_b[k].abs() > G_MAX_DB {
                ceiling_hits.push((name.to_string(), k));
                if raw[k].abs() <= G_MAX_DB {
                    matrix_pushes_over.push((name.to_string(), k));
                }
            }
        }
        println!();
    }

    // ── ΤΡΙΑ ΣΥΝΟΛΙΚΑ ΝΟΥΜΕΡΑ ──
    let total = all_raw.len();
    let n_files = waking.len();
    println!("── ΣΥΝΟΛΙΚΑ (δικό μας υλικό, {n_files} αρχεία × 8 μπάντες = {total}) ──");
    println!(
        "Μπάντες στο ταβάνι (|POST_B| > {G_MAX_DB}): {}/{total}",
        ceiling_hits.len()
    );
    if !ceiling_hits.is_empty() {
        for (name, k) in &ceiling_hits {
            println!("    {name}  band {k} ({} Hz)", LTASS_CFS[*k]);
        }
    }

    let max_abs_raw = all_raw.iter().cloned().fold(0.0_f32, |a, b| a.max(b.abs()));
    println!("Μεγαλύτερη ζητούμενη |RAW| διόρθωση στο δείγμα: {max_abs_raw:.3} dB");

    let mut abs_sorted: Vec<f32> = all_raw.iter().map(|v| v.abs()).collect();
    abs_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = if abs_sorted.len() % 2 == 1 {
        abs_sorted[abs_sorted.len() / 2]
    } else {
        let mid = abs_sorted.len() / 2;
        (abs_sorted[mid - 1] + abs_sorted[mid]) / 2.0
    };
    println!("Διάμεσος |RAW| διόρθωσης: {median:.3} dB");

    println!(
        "\n⚠ ΚΡΙΣΙΜΟ — ο πίνακας ανεβάζει ζητούμενη τιμή πάνω από το ταβάνι ενώ χωρίς αυτόν θα περνούσε: {}/{total}",
        matrix_pushes_over.len()
    );
    for (name, k) in &matrix_pushes_over {
        println!("    {name}  band {k} ({} Hz)", LTASS_CFS[*k]);
    }
}
