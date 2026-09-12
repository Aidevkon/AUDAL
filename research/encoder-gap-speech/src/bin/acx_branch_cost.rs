//! ΜΕΤΡΗΣΗ: πόσο κοστίζει ο AcxCheckAnalyzer στο trunk pass, και ποιο interior
//! δίνει η ΑΛΥΣΙΔΑ (dump μετά από decode) έναντι του εργαλείου που διαβάζει το
//! αρχείο απευθείας (corpus_floor_filter).
//!
//! Καλεί τις ΠΡΑΓΜΑΤΙΚΕΣ δημόσιες εισόδους — `run_trunk_pass` και
//! `run_trunk_pass_with_acx` — όχι re-implementation.
//!
//! ⚠ ΤΙ ΔΕΝ ΑΠΟΔΕΙΚΝΥΕΙ: ότι ο κλάδος του `execute_streaming_plan` διαλέγει
//! σωστά. Καλεί τις δύο εισόδους ΑΠΕΥΘΕΙΑΣ, παρακάμπτοντας την απόφαση. Ο
//! κλάδος δεν είναι παρατηρήσιμος από έξω: το `DspOutput` (operator.rs:174-184)
//! δεν φέρει το trunk_report. Αυτό μετριέται ξεχωριστά, με probe.
//!
//! ΧΡΗΣΗ: cargo run --release --bin acx_branch_cost -- <audio> [runs]

use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let audio = args.get(1).expect("usage: acx_branch_cost <audio> [runs]");
    let runs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);

    // Το dump που βλέπει η ζωντανή διαδρομή: ό,τι γράφει το pass0 του executor.
    let dump = std::env::temp_dir().join("acx_branch_cost.raw");
    let dump_s = dump.to_string_lossy().into_owned();
    let (metrics, _dec) = m0d::dsp::input_lufs::pass0_decode_to_dump(
        std::path::Path::new(audio),
        &dump_s,
    )
    .expect("pass0 decode");

    let edge_s = lineos_types::presets::ACX
        .room_tone_max_s
        .expect("ACX ορίζει room_tone_max_s");
    let limit_db = lineos_types::presets::ACX
        .max_noise_floor_db
        .expect("ACX ορίζει max_noise_floor_db");

    println!("── acx_branch_cost ────────────────────────────────────────────");
    println!("υλικό      {audio}");
    println!("input LUFS {:?}", metrics.integrated_lufs);
    println!("edge_s {edge_s} · limit {limit_db} dB (και τα δύο από presets.rs)");
    println!("τρεξίματα ανά κλάδο: {runs}\n");

    // ── Β: ΧΩΡΙΣ analyzer (ό,τι κάνει σήμερα κάθε preset εκτός acx) ──
    let mut t_without = Vec::new();
    let mut b_without = 0usize;
    for _ in 0..runs {
        let t0 = Instant::now();
        let r = sp314_orchestrator::trunk_pass::run_trunk_pass(&dump, false)
            .expect("trunk pass");
        t_without.push(t0.elapsed().as_secs_f64() * 1000.0);
        b_without = r.boundaries.len();
        assert!(r.acx.is_none(), "χωρίς analyzer το acx πρέπει None");
        assert!(r.acx_interior_noise_floor.is_none());
    }

    // ── Α: ΜΕ analyzer ──
    let mut t_with = Vec::new();
    let mut b_with = 0usize;
    let mut interior = None;
    let mut absolute = None;
    for _ in 0..runs {
        let t0 = Instant::now();
        let r = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(&dump, false, edge_s)
            .expect("trunk pass with acx");
        t_with.push(t0.elapsed().as_secs_f64() * 1000.0);
        b_with = r.boundaries.len();
        interior = r.acx_interior_noise_floor;
        absolute = r.acx.map(|a| (a.noise_floor_db, a.quietest_window_start_frame));
    }

    // ── ΤΑ ΤΡΙΑ ΜΕΓΕΘΗ ──
    println!("ΒΗΜΑ 1 — τι δίνει κάθε κλάδος");
    println!("  Β (run_trunk_pass)          acx=None  interior=None  boundaries={b_without}");
    println!(
        "  Α (run_trunk_pass_with_acx) acx={:?}  interior={:?}  boundaries={b_with}",
        absolute, interior
    );
    println!(
        "  boundaries ταυτόσημα: {}",
        if b_with == b_without { "ΝΑΙ" } else { "ΟΧΙ ⚠" }
    );

    // ── ΤΟ ΚΟΣΤΟΣ ──
    let fmt = |v: &Vec<f64>| {
        v.iter()
            .map(|x| format!("{x:.1}"))
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let mean = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len() as f64;
    let spread = |v: &Vec<f64>| {
        v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
    };

    let m_with = mean(&t_with);
    let m_without = mean(&t_without);
    let delta = m_with - m_without;
    // Διάρκεια από το ΙΔΙΟ dump που μέτρησε ο trunk: 48k stereo f32 LE.
    let audio_sec = std::fs::metadata(&dump).map(|m| m.len() as f64 / (48_000.0 * 2.0 * 4.0)).unwrap_or(f64::NAN);

    println!("\nΒΗΜΑ 2 — κόστος (ms, ΜΟΝΟ το run_trunk_*)");
    println!("  ΧΩΡΙΣ  {}", fmt(&t_without));
    println!("  ΜΕ     {}", fmt(&t_with));
    println!("  μέσοι  ΧΩΡΙΣ {m_without:.1}  ·  ΜΕ {m_with:.1}");
    println!("  διαφορά {delta:.1} ms  =  {:.2}% του ΧΩΡΙΣ", delta / m_without * 100.0);
    println!("  ήχος {audio_sec:.1} s ⇒ {:.3} ms ανά δευτερόλεπτο ήχου", delta / audio_sec);
    println!(
        "  διακύμανση ΧΩΡΙΣ {:.1} ms · ΜΕ {:.1} ms",
        spread(&t_without),
        spread(&t_with)
    );
    let max_spread = spread(&t_without).max(spread(&t_with));
    println!(
        "  ⇒ {}",
        if max_spread >= delta.abs() {
            "Η ΔΙΑΚΥΜΑΝΣΗ ΚΑΛΥΠΤΕΙ ΤΗ ΔΙΑΦΟΡΑ — η μέτρηση ΔΕΝ διακρίνει"
        } else {
            "η διαφορά ξεπερνά τη διακύμανση"
        }
    );

    let _ = std::fs::remove_file(&dump);
}
