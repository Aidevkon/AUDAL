//! RECON 2026-09-18: πόσο κινεί το RMS και το LUFS το spacing και το
//! channel conform. Read-only, ΜΗΔΕΝ αλλαγή στην παραγωγή, ΜΗΔΕΝ γράψιμο
//! αρχείου παραγωγής — μόνο ανάγνωση mp3 + ΠΡΑΓΜΑΤΙΚΑ όργανα μέτρησης.
//!
//! ΓΙΑΤΙ: το PRD v7.1 βάζει το Στάδιο 13 (level conform) να διαλέγει
//! στόχο από το set_reference, υπολογισμένο στο Πέρασμα Β από τις
//! αρχικές μετρήσεις — ΠΡΙΝ τα Στάδια 11 (spacing) και 12 (channel) που
//! τρέχουν ΜΕΤΑ, αλλάζοντας διάρκεια και κανάλια. Υπόθεση: ακυρώνουν τη
//! μέτρηση που το 13 χρησιμοποιεί.
//!
//! ΤΟ ΟΡΓΑΝΟ (διαβασμένο, όχι υποτιθέμενο):
//!   AcxCheckAnalyzer::finish().rms_db — ΟΛΟΚΛΗΡΟ το αρχείο, ΧΩΡΙΣ
//!   χρονική στάθμιση, ΧΩΡΙΣ εξαίρεση dead air (acx_check.rs:12-14: "RMS
//!   whole-file, unweighted, after DC removal", ένα πέρασμα
//!   E[x^2]-mean^2). Τροφοδοτείται με το mono downmix (l+r)*0.5
//!   (trunk_pass.rs:722, ΙΔΙΟΣ τύπος με scout_scanner.rs:30). Η σιωπή
//!   ΔΕΝ εξαιρείται — μετράει ΙΣΑ με τον λόγο.
//!   measure_integrated_lufs — ITU-R BS.1770-4: απόλυτο gate στα -70
//!   LUFS (gating.rs:7, ABSOLUTE_GATE_DB) + σχετικό gate στα -10 LU κάτω
//!   από τον μέσο όρο χωρίς gate (gating.rs:8, RELATIVE_GATE_LU), ανά
//!   block 400ms/hop 100ms (75% overlap). Ήσυχα άκρα κάτω από -70 LUFS
//!   ή >10dB κάτω από το επίπεδο αφήγησης αποκλείονται από την
//!   ολοκλήρωση σχεδόν εξ ολοκλήρου.
//!
//! ΧΡΗΣΗ: cargo run --release --bin rms_lufs_edge_sensitivity
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use sp314_dsp::analysis::stereo::stereo_correlation;
use sp314_dsp::metering::lufs::measure_integrated_lufs;

const SR: u32 = 48_000;
const TRIM_SECS: f32 = 2.0;
const EDGE_SECS: f32 = 2.0;

fn read_dump_stereo(path: &str) -> (Vec<f32>, Vec<f32>) {
    let buf = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let n = buf.len() / 8;
    let mut l = Vec::with_capacity(n);
    let mut r = Vec::with_capacity(n);
    for i in 0..n {
        let base = i * 8;
        l.push(f32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]));
        r.push(f32::from_le_bytes([buf[base + 4], buf[base + 5], buf[base + 6], buf[base + 7]]));
    }
    (l, r)
}

fn mono_of(l: &[f32], r: &[f32]) -> Vec<f32> {
    l.iter().zip(r.iter()).map(|(&a, &b)| (a + b) * 0.5).collect()
}

/// ΤΟ ΠΡΑΓΜΑΤΙΚΟ όργανο, ΟΛΟΚΛΗΡΟ το κομμάτι που δίνεται.
fn acx_rms_db(mono: &[f32]) -> f32 {
    let mut a = AcxCheckAnalyzer::new(SR);
    a.feed_chunk(mono);
    a.finish().rms_db
}

/// "RMS ως έχει" — pooled ενέργεια ΑΝΑ ΚΑΝΑΛΙ, χωρίς κανένα downmix
/// (τροφοδοτεί το ΙΔΙΟ όργανο διαδοχικά με L και μετά R — sum/sum_sq
/// συσσωρεύονται και στα δύο κανάλια εξίσου, δίνοντας
/// sqrt(E[L^2]+E[R^2])/2) μετά DC-removal στο combined stream).
fn stereo_pooled_rms_db(l: &[f32], r: &[f32]) -> f32 {
    let mut a = AcxCheckAnalyzer::new(SR);
    a.feed_chunk(l);
    a.feed_chunk(r);
    a.finish().rms_db
}

fn main() {
    let dir = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    let books = [
        ("secretgarden", "secretgarden_01_burnett.mp3"),
        ("dracula", "dracula_01_stoker.mp3"),
        ("count_of_monte_cristo", "count_of_monte_cristo_001_dumas.mp3"),
        ("peterpan", "peterpan_01_barrie.mp3"),
        ("anne_of_green_gables", "anne_of_green_gables_01_montgomery.mp3"),
        ("tale_of_two_cities", "tale_of_two_cities_01_dickens.mp3"),
        ("adventurespinocchio", "adventurespinocchio_01_collodi.mp3"),
        ("huckfinn", "huckfinn_01_twain_apc.mp3"),
        ("janeeyre", "janeeyre_01_bronte.mp3"),
    ];

    let trim_samples = (TRIM_SECS * SR as f32) as usize;
    let edge_samples = (EDGE_SECS * SR as f32) as usize;

    println!("SR={SR} TRIM_SECS={TRIM_SECS} EDGE_SECS={EDGE_SECS}\n");
    println!("=== (1) ΩΣ ΕΧΕΙ vs ΑΚΡΑ ΚΟΜΜΕΝΑ 2s — RMS/LUFS ===\n");

    let mut n_stereo = 0usize;
    let mut channel_rows: Vec<(String, f32, f32, f32, f32)> = Vec::new();

    for (label, fname) in books {
        let path = format!("{dir}/{fname}");
        let dump = format!("/tmp/rmslufs_{label}.raw");
        let (_metrics, _dec) = m0d::dsp::input_lufs::pass0_decode_to_dump(
            std::path::Path::new(&path),
            &dump,
        )
        .unwrap_or_else(|e| panic!("decode {path}: {e}"));
        let (l, r) = read_dump_stereo(&dump);
        let _ = std::fs::remove_file(&dump);
        let n = l.len();
        if n <= 2 * trim_samples {
            println!("{label}: ΠΟΛΥ ΚΟΝΤΟ για κόψιμο {TRIM_SECS}s ανά άκρο — παραλείπεται");
            continue;
        }

        let mono_full = mono_of(&l, &r);
        let rms_full = acx_rms_db(&mono_full);
        let lufs_full = measure_integrated_lufs(&l, &r);

        let l_trim = &l[trim_samples..n - trim_samples];
        let r_trim = &r[trim_samples..n - trim_samples];
        let mono_trim = mono_of(l_trim, r_trim);
        let rms_trim = acx_rms_db(&mono_trim);
        let lufs_trim = measure_integrated_lufs(l_trim, r_trim);

        let rms_diff = rms_trim - rms_full;
        let lufs_diff = lufs_trim - lufs_full;

        // Ησυχία στα άκρα, dBFS, ΙΔΙΟ όργανο σε 2s slice από κάθε άκρο.
        let start_edge = mono_of(&l[..edge_samples.min(n)], &r[..edge_samples.min(n)]);
        let end_edge = mono_of(&l[n - edge_samples.min(n)..], &r[n - edge_samples.min(n)..]);
        let edge_start_db = acx_rms_db(&start_edge);
        let edge_end_db = acx_rms_db(&end_edge);

        println!(
            "{:<24} RMS ως-έχει={:>7.3}dB κομμένο={:>7.3}dB Δ={:>+6.3}dB | LUFS ως-έχει={:>7.3} κομμένο={:>7.3} Δ={:>+6.3} | άκρα(dBFS) αρχή={:>7.2} τέλος={:>7.2}",
            label, rms_full, rms_trim, rms_diff, lufs_full, lufs_trim, lufs_diff, edge_start_db, edge_end_db
        );

        // (3) κανάλι — μόνο αν πραγματικά στέρεο (L != R σε κάποιο δείγμα).
        let is_stereo = l.iter().zip(r.iter()).any(|(&a, &b)| (a - b).abs() > 1e-6);
        if is_stereo {
            n_stereo += 1;
            let stereo_rms = stereo_pooled_rms_db(&l, &r);
            let mono_rms = rms_full; // ήδη υπολογισμένο, ίδιος τύπος downmix με παραγωγή
            let corr = stereo_correlation(&l, &r);
            channel_rows.push((label.to_string(), stereo_rms, mono_rms, mono_rms - stereo_rms, corr));
        }
    }

    println!("\n=== (3) ΚΑΝΑΛΙ — RMS ανά-κανάλι (pooled) vs mono downmix (l+r)*0.5, ΚΑΙ global_phase_correlation ===\n");
    if n_stereo == 0 {
        println!("ΚΑΝΕΝΑ από τα εννιά αρχεία δεν είναι στέρεο (L≡R παντού) — τέλος για το (3).");
    } else {
        println!("{n_stereo}/9 αρχεία είναι πραγματικά στέρεο (L≠R κάπου). Θεωρητικό: 0dB για ταυτόσημα κανάλια, -3dB για ασύσχετα, ΜΕ ΤΟΝ ΠΡΑΓΜΑΤΙΚΟ τύπο downmix (l+r)*0.5 — ΟΧΙ +3dB/0dB της σύμβασης (l+r)/sqrt(2).\n");
        for (label, stereo_rms, mono_rms, diff, corr) in &channel_rows {
            println!(
                "{:<24} στέρεο(pooled)={:>7.3}dB mono(downmix)={:>7.3}dB Δ={:>+6.3}dB  global_phase_correlation={:>6.4}",
                label, stereo_rms, mono_rms, diff, corr
            );
        }
    }
}
