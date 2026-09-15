//! ΑΚΡΟΑΣΗ [4]: η λωρίδα «σίγουρα φωνή» σε πραγματικό υλικό.
//!
//! ⚠ ΤΙ ΚΑΝΕΙ ΚΑΙ ΤΙ ΔΕΝ: το `restoration_enabled` ΔΕΝ γυρίζει από έξω — το
//! `StreamingPlan` (operator.rs:229-241) δεν το φέρει, και ο executor το
//! καρφώνει `true` (executor.rs:410). Άρα ΔΕΝ περνάει από το
//! `execute_streaming_plan`. Καλεί ΤΗΝ ΙΔΙΑ ζωντανή συνάρτηση
//! `run_streaming_pipeline_with_timeline`, δύο φορές, με ΜΟΝΗ διαφορά τη
//! σημαία — και με boundaries από τον ΠΡΑΓΜΑΤΙΚΟ Scout (`run_trunk_pass`).
//! ⇒ Ό,τι διαφέρει ανάμεσα στα δύο αρχεία είναι η λωρίδα, και τίποτε άλλο.
//!
//! ΟΡΓΑΝΑ: `run_trunk_pass_with_acx` και `AcxCheckAnalyzer` — τα υπάρχοντα.
//! ΚΑΜΙΑ νέα υλοποίηση μέτρησης.
//!
//! ΧΡΗΣΗ: cargo run --release --bin expander_audition -- <audio>...

use sp314_orchestrator::streaming_pipeline::{
    run_streaming_pipeline_with_timeline, StreamingConfig, TimelinePlan,
};

const OUT_DIR: &str = "/tmp/expander-audition";

struct Measured {
    interior: Option<f32>,
    absolute: Option<f32>,
    absolute_at_s: Option<f64>,
    lufs: f32,
    true_peak: f32,
    lra: f32,
    crest: f32,
    dyn_range: f32,
    frames: usize,
    spectral: [f32; 8],
}

fn measure(path: &str, tag: &str) -> (Measured, Vec<f32>) {
    let dump = format!("/tmp/expander_audition_{tag}.raw");
    let (m, _d) = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .expect("pass0");
    let edge = lineos_types::presets::ACX.room_tone_max_s.unwrap();
    let r = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
        std::path::Path::new(&dump),
        false,
        edge,
    )
    .expect("trunk");
    // ΤΟ ΙΔΙΟ dump που μόλις μέτρησε ο trunk — 48k stereo f32 LE. ΟΧΙ δεύτερο
    // decode: το `decode_raw_interleaved` έχει όριο διάρκειας
    // (DurationExceeded) και απορρίπτει αρχεία αφήγησης.
    let mono = read_dump_mono(&dump);
    let out = Measured {
        interior: r.acx_interior_noise_floor.map(|(db, _)| db),
        absolute: r.acx.and_then(|a| a.noise_floor_db),
        absolute_at_s: r
            .acx
            .and_then(|a| a.quietest_window_start_frame)
            .map(|f| f as f64 / 48_000.0),
        lufs: r.integrated_lufs.unwrap_or(-144.0),
        true_peak: m.true_peak_dbtp,
        lra: r.lra,
        crest: r.crest_db,
        dyn_range: r.dynamic_range_db,
        frames: mono.len(),
        spectral: r.spectral_profile_db,
    };
    let _ = std::fs::remove_file(&dump);
    (out, mono)
}

/// Mono downmix από dump 48k stereo f32 LE — ΙΔΙΟΣ τύπος με το
/// trunk_pass.rs:509 `mono[i] = (l + r) * 0.5`.
fn read_dump_mono(dump: &str) -> Vec<f32> {
    let bytes = std::fs::read(dump).expect("read dump");
    let n = bytes.len() / 4;
    let mut inter = Vec::with_capacity(n);
    for i in 0..n {
        inter.push(f32::from_le_bytes([
            bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3],
        ]));
    }
    inter.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect()
}

/// RMS ανά 100 ms σε dBFS.
fn rms_100ms(x: &[f32], sr: u32) -> Vec<f32> {
    let w = (sr / 10) as usize;
    x.chunks(w)
        .map(|c| {
            let e = c.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / c.len() as f64;
            if e < 1e-20 {
                -200.0
            } else {
                (10.0 * e.log10()) as f32
            }
        })
        .collect()
}

fn render(input: &str, tag: &str, restoration: bool, limit: Option<f32>, floor: Option<f32>,
    split: Option<f32>)
    -> String
{
    let out = format!("{OUT_DIR}/{tag}.wav");

    // Boundaries από τον ΠΡΑΓΜΑΤΙΚΟ Scout.
    let dump = format!("/tmp/expander_audition_src_{tag}.raw");
    let _ = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(input), &dump)
        .expect("pass0");
    let report = sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(&dump), false)
        .expect("trunk");
    let boundaries = report.boundaries.clone();

    let mut db = sp314_nodes::topology::DspTopologyBuilder::new("ducking_fallback_topology");
    let d_in = db.add_node("in", "Input", serde_json::json!({}));
    let d_gain = db.add_node("duck_gain", "Gain",
        serde_json::json!({ "gain": 1.0, "glide_ms": 300.0 }));
    let d_out = db.add_node("out", "Output", serde_json::json!({}));
    db.connect(&d_in, &d_gain);
    db.connect(&d_gain, &d_out);
    let topology = db.build();

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    let reader = m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input)).unwrap();
    let _w = m0d::dsp::orchestrator::nmf_worker::spawn(reader, 48_000, rx_job, tx_res);
    let (_sent, flagged) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);

    let decoder = sp314_orchestrator::decode_provider::DumpDecodeProvider::new(dump.clone());
    run_streaming_pipeline_with_timeline(
        decoder,
        &out,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: 48_000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 0.501,
            pre_gain_linear: 1.0, // ΙΔΙΟ και στα δύο σκέλη
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            max_true_peak_db: lineos_types::presets::ACX.max_true_peak_db,
            max_noise_floor_db: limit,
            input_interior_floor_db: floor,
            quiet_window_split_dbfs: split,
            input_fundamental: None,
            intent_dynamics: None,
            restoration_enabled: restoration,
        },
        TimelinePlan { boundaries, flagged_indices: flagged, pre_analysis: None },
        rx_res,
    )
    .expect("render");
    let _ = std::fs::remove_file(&dump);
    out
}

fn main() {
    std::fs::create_dir_all(OUT_DIR).expect("mkdir");
    // ΠΡΟΑΙΡΕΤΙΚΟ ΔΕΥΤΕΡΟ ΟΡΙΣΜΑ: κατώφλι-υποψήφιος σε dBFS.
    //
    // ⚠ ΠΩΣ ΕΠΙΒΑΛΛΕΤΑΙ ΧΩΡΙΣ ΝΑ ΑΓΓΙΧΤΕΙ ΠΑΡΑΓΩΓΗ: η `expander_threshold_db`
    // επιστρέφει ΤΟ ΟΡΙΟ όταν το πάτωμα είναι πάνω του. Δίνοντας
    // max_noise_floor_db = <υποψήφιος> και input_interior_floor_db =
    // <υποψήφιος>+1, η ΙΔΙΑ συνάρτηση επιστρέφει ακριβώς τον υποψήφιο.
    // Η συνάρτηση χρησιμοποιείται ως ΜΟΧΛΟΣ· τίποτα δεν άλλαξε στον κώδικα.
    let mut files: Vec<String> = std::env::args().skip(1).collect();
    let candidate: Option<f32> = files.last().and_then(|s| s.parse::<f32>().ok());
    if candidate.is_some() { files.pop(); }
    assert!(!files.is_empty(), "usage: expander_audition <audio>... [threshold_db]");

    let limit = lineos_types::presets::ACX.max_noise_floor_db.unwrap();
    let edge = lineos_types::presets::ACX.room_tone_max_s.unwrap();
    println!("limit {limit} dBFS · edge {edge} s (presets.rs) · έξοδος {OUT_DIR}\n");

    for path in &files {
        let stem = std::path::Path::new(path)
            .file_stem().unwrap().to_string_lossy().into_owned();
        println!("═══════════════════════════════════════════════════════════");
        println!("{stem}");

        // ── ΒΗΜΑ 0: ο Scout ──
        let src_dump = format!("/tmp/expander_audition_scout_{stem}.raw");
        let _ = m0d::dsp::input_lufs::pass0_decode_to_dump(
            std::path::Path::new(path), &src_dump).expect("pass0");
        let rep = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
            std::path::Path::new(&src_dump), false, edge).expect("trunk");
        let src_interior = rep.acx_interior_noise_floor.map(|(db, _)| db);
        let flagged = lineos_corpus::scout::flag_escalation_candidates(&rep.boundaries);

        println!("ΒΗΜΑ 0 — Ο SCOUT");
        println!("  τμήματα: {} · flagged: {}", rep.boundaries.len(), flagged.len());
        let mut speech_unflagged = 0usize;
        for (i, b) in rep.boundaries.iter().enumerate() {
            let f = flagged.contains(&i);
            let lane = matches!(b.segment_type, lineos_corpus::scout::SegmentType::Speech) && !f;
            if lane { speech_unflagged += 1; }
            if i < 6 || lane {
                println!("   [{i:3}] {:>6.1}–{:<7.1}s {:?}  leaning {:.3} conf {:.3}  {}",
                    b.start_sec, b.end_sec, b.segment_type, b.avg_leaning, b.avg_confidence,
                    if f { "FLAGGED→stems" } else if lane { "→ΛΩΡΙΔΑ" } else { "—" });
            }
        }
        println!("  τμήματα που πάνε στη ΛΩΡΙΔΑ: {speech_unflagged}");
        let src_quietest = rep.quietest_active_window_dbfs;
        let src_split = rep.quiet_window_split_dbfs;
        println!("  ΤΟΜΗ ΚΑΤΑΝΟΜΗΣ (Otsu, από το trunk pass): {src_split:?}");
        if let (Some(t), Some(f)) = (src_split, src_interior) {
            println!("     τομή πάνω από interior: {:.2} dB · ΠΡΟΒΛΕΨΗ μείωσης {:.2} dB",
                t - f, ((t - f) * 0.5).min(12.0));
        }
        println!("  ΤΑ ΔΥΟ ΦΡΑΓΜΑΤΑ: κάτω(interior) {src_interior:?} · άνω(quietest speech) {src_quietest:?}");
        if let (Some(lo), Some(hi)) = (src_interior, src_quietest) {
            println!("     απόσταση {:.2} dB · μέσο {:.3} dBFS", hi - lo, (hi + lo) / 2.0);
        }
        println!("  interior πηγής: {src_interior:?} ⇒ expander {}",
            if sp314_orchestrator::streaming_pipeline::expander_threshold_db(
                Some(limit), src_interior, src_split).is_some() { "ΤΡΕΧΕΙ" } else { "ΔΕΝ τρέχει" });
        let _ = std::fs::remove_file(&src_dump);

        if speech_unflagged == 0 {
            println!("  ⚠ ΜΗΔΕΝ τμήματα στη λωρίδα — το αρχείο ΔΕΝ μετράει γι' αυτό το τεστ.\n");
            continue;
        }

        // ── ΒΗΜΑ 1: τα δύο renders ──
        // ΣΕΝΤΙΝΕΛΑ 0 = ΜΟΝΟ ΤΑ ΦΡΑΓΜΑΤΑ, χωρίς render. Η σάρωση των εννιά
        // WAKING χρειάζεται μόνο το trunk pass· τα renders θα ήταν ώρες.
        if candidate == Some(0.0) {
            println!();
            continue;
        }

        // ΜΗΔΕΝ ΜΟΧΛΟΣ. Το κατώφλι έρχεται από την ΠΑΡΑΓΩΓΗ: την τομή που
        // υπολόγισε το ίδιο trunk pass πάνω στην κατανομή του αρχείου.
        let (pass_limit, pass_floor) = (Some(limit), src_interior);
        let tag = "otsu";
        let off = render(path, &format!("{stem}__{tag}__OFF"), false, pass_limit, pass_floor, src_split);
        let on  = render(path, &format!("{stem}__{tag}__ON"),  true,  pass_limit, pass_floor, src_split);

        // ── ΒΗΜΑ 2: ο πίνακας ──
        let (mo, mono_off) = measure(&off, &format!("{stem}_{tag}_off"));
        let (mn, mono_on)  = measure(&on,  &format!("{stem}_{tag}_on"));
        let d = |a: f32, b: f32| b - a;
        println!("\nΒΗΜΑ 2 — Ο ΠΙΝΑΚΑΣ");
        println!("  {:<22}{:>12}{:>12}{:>12}", "", "OFF", "ON", "Δ");
        println!("  {:<22}{:>12.3}{:>12.3}{:>12.4}", "interior dBFS",
            mo.interior.unwrap_or(f32::NAN), mn.interior.unwrap_or(f32::NAN),
            d(mo.interior.unwrap_or(0.0), mn.interior.unwrap_or(0.0)));
        println!("  {:<22}{:>12.3}{:>12.3}{:>12.4}", "absolute dBFS",
            mo.absolute.unwrap_or(f32::NAN), mn.absolute.unwrap_or(f32::NAN),
            d(mo.absolute.unwrap_or(0.0), mn.absolute.unwrap_or(0.0)));
        println!("  {:<22}{:>12.1}{:>12.1}", "  θέση απόλυτου s",
            mo.absolute_at_s.unwrap_or(f64::NAN), mn.absolute_at_s.unwrap_or(f64::NAN));
        println!("  {:<22}{:>12.3}{:>12.3}{:>12.4}", "LUFS", mo.lufs, mn.lufs, d(mo.lufs, mn.lufs));
        println!("  {:<22}{:>12.3}{:>12.3}{:>12.4}", "true peak dBTP",
            mo.true_peak, mn.true_peak, d(mo.true_peak, mn.true_peak));
        println!("  {:<22}{:>12.3}{:>12.3}{:>12.4}", "LRA LU", mo.lra, mn.lra, d(mo.lra, mn.lra));
        println!("  {:<22}{:>12.3}{:>12.3}{:>12.4}", "crest dB", mo.crest, mn.crest, d(mo.crest, mn.crest));
        println!("  {:<22}{:>12.3}{:>12.3}{:>12.4}", "dyn range dB",
            mo.dyn_range, mn.dyn_range, d(mo.dyn_range, mn.dyn_range));
        println!("  {:<22}{:>12}{:>12}   {}", "frames", mo.frames, mn.frames,
            if mo.frames == mn.frames { "ΤΑΥΤΟΣΗΜΗ" } else { "⚠ ΔΙΑΦΕΡΕΙ" });

        // ── ΒΗΜΑ 3.1: φάσμα ──
        println!("\nΒΗΜΑ 3.1 — ΦΑΣΜΑ 8 ΜΠΑΝΤΩΝ (dB)");
        print!("  OFF ");
        for v in mo.spectral { print!("{v:8.2}"); }
        print!("\n  ON  ");
        for v in mn.spectral { print!("{v:8.2}"); }
        print!("\n  Δ   ");
        for k in 0..8 { print!("{:8.3}", mn.spectral[k] - mo.spectral[k]); }
        println!();

        // ── ΒΗΜΑ 3.2 + 3.3 ──
        let a = rms_100ms(&mono_off, 48_000);
        let b = rms_100ms(&mono_on, 48_000);
        let n = a.len().min(b.len());

        println!("\nΒΗΜΑ 3.2 — ΚΑΤΑΝΟΜΗ ΗΣΥΧΩΝ ΠΑΡΑΘΥΡΩΝ (< −40 dBFS), 100 ms");
        let edges = [-200.0, -90.0, -80.0, -70.0, -60.0, -50.0, -40.0];
        for w in edges.windows(2) {
            let ca = a[..n].iter().filter(|v| **v >= w[0] && **v < w[1]).count();
            let cb = b[..n].iter().filter(|v| **v >= w[0] && **v < w[1]).count();
            println!("  [{:>6.0}, {:>4.0})  OFF {:>6}  ON {:>6}  Δ {:>+6}",
                w[0], w[1], ca, cb, cb as i64 - ca as i64);
        }

        println!("\nΒΗΜΑ 3.3 — |ON − OFF| ΣΤΟΝ ΧΡΟΝΟ");
        let mut diffs: Vec<(usize, f32)> = (0..n)
            .map(|i| (i, (b[i] - a[i]).abs()))
            .filter(|(_, d)| *d > 0.0)
            .collect();
        println!("  παράθυρα με διαφορά: {}/{n}", diffs.len());
        diffs.sort_by(|x, y| y.1.total_cmp(&x.1));
        println!("  ΟΙ ΤΡΕΙΣ ΜΕΓΑΛΥΤΕΡΕΣ — ΕΚΕΙ ΝΑ ΑΚΟΥΣΕΙ ΠΡΩΤΑ:");
        for (i, dv) in diffs.iter().take(3) {
            println!("   {:>8.1}s   Δ {:>7.2} dB   (OFF {:.1} → ON {:.1})",
                *i as f32 / 10.0, dv, a[*i], b[*i]);
        }
        // Πού εμφανίζεται η διαφορά: πάνω ή κάτω από −40;
        let loud = diffs.iter().filter(|(i, _)| a[*i] >= -40.0).count();
        println!("  από αυτά, σε παράθυρα ΠΑΝΩ από −40 dBFS: {loud}");
        println!("\n  OFF: {off}\n  ON : {on}\n");
    }
}
