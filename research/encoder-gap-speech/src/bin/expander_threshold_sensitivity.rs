//! MEASURE: πόσο ευαίσθητο είναι το αποτέλεσμα του expander στην ακριβή
//! θέση της τομής Otsu. ΜΗΔΕΝ αλλαγή παραγωγής — χρησιμοποιεί τον
//! ΥΠΑΡΧΟΝΤΑ μοχλό (`expander_threshold_db` επιστρέφει `quiet_window_
//! split_dbfs` όταν `input_interior_floor_db > max_noise_floor_db`),
//! γραμμένο στο `expander_audition.rs` στις 14/09 και ποτέ χρησιμοποιημένο.
//!
//! ⚠ ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ: `measure`/`render`/`rms_100ms`/`read_dump_mono`
//! είναι αντίγραφο των ομώνυμων στο `expander_audition.rs` — bin-crates
//! δεν μοιράζονται ιδιωτικές functions μεταξύ τους σε αυτό το workspace.
//! ΚΑΜΙΑ αλλαγή στο πρωτότυπο· αν αλλάξει εκεί, πρέπει να αλλάξει κι εδώ.
//!
//! ΤΟ ΜΟΝΟ ΠΟΥ ΔΙΑΦΕΡΕΙ ΑΝΑ ΤΡΕΞΙΜΟ: το `quiet_window_split_dbfs` που
//! περνάει στο `StreamingConfig` — `otsu + offset`, offset ∈ {-4,-2,0,2,4}.
//! Το `max_noise_floor_db`/`input_interior_floor_db` μένουν ΠΡΑΓΜΑΤΙΚΑ
//! (από το trunk pass) σε όλο το sweep — η πύλη «τρέχει ο expander;»
//! παραμένει ίδια με την παραγωγή, μόνο η ΘΕΣΗ της τομής αλλάζει.
//!
//! ═══ Η ΠΡΟΒΛΕΨΗ, ΓΡΑΜΜΕΝΗ ΠΡΙΝ ΤΡΕΞΕΙ ΟΤΙΔΗΠΟΤΕ ═══
//! Συμφωνώ εν μέρει με τη δοθείσα πρόβλεψη (κλίση ~0.5dB/dB, monte_cristo
//! περνάει στο +2 και όχι στο -2, huckfinn αδιάφορο). ΔΙΑΦΩΝΩ στο ΣΧΗΜΑ:
//! δεν περιμένω ΓΡΑΜΜΙΚΗ κλίση σε όλο το εύρος ±4dB. Οι ήσυχες παύσεις
//! μειώνονται με φθίνουσα απόδοση καθώς η τομή κατεβαίνει (otsu-4/-2 —
//! ήδη λίγα παράθυρα εκεί κάτω), ενώ καθώς η τομή ανεβαίνει (otsu+2/+4)
//! περιμένω ΑΠΟΤΟΜΗ αύξηση στα παράθυρα πάνω από -40dBFS — δηλαδή η
//! καμπύλη θα είναι ασύμμετρη, πιο απότομη προς τα πάνω. Το άγνωστο (πόση
//! ομιλία πιάνει στο +4) το μοιράζομαι αυτούσιο — δεν έχω πρόβλεψη αριθμού,
//! μόνο κατεύθυνση. Και θυμάμαι το c161dc9: η αριθμητική πρόβλεψη μπορεί
//! να αστοχήσει κατά dB, όχι μόνο κατά ποσοστό.
//!
//! ΧΡΗΣΗ: cargo run --release --bin expander_threshold_sensitivity

use sp314_orchestrator::streaming_pipeline::{
    run_streaming_pipeline_with_timeline, StreamingConfig, TimelinePlan,
};

const OUT_DIR: &str = "/tmp/expander-threshold-sensitivity";
const OFFSETS: [f32; 5] = [-4.0, -2.0, 0.0, 2.0, 4.0];

struct Measured {
    interior: Option<f32>,
    absolute: Option<f32>,
    lufs: f32,
    frames: usize,
}

fn measure(path: &str, tag: &str) -> (Measured, Vec<f32>) {
    let dump = format!("/tmp/ets_{tag}.raw");
    let _ = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .expect("pass0");
    let edge = lineos_types::presets::ACX.room_tone_max_s.unwrap();
    let r = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
        std::path::Path::new(&dump),
        false,
        edge,
    )
    .expect("trunk");
    let mono = read_dump_mono(&dump);
    let out = Measured {
        interior: r.acx_interior_noise_floor.map(|(db, _)| db),
        absolute: r.acx.and_then(|a| a.noise_floor_db),
        lufs: r.integrated_lufs.unwrap_or(-144.0),
        frames: mono.len(),
    };
    let _ = std::fs::remove_file(&dump);
    (out, mono)
}

/// ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ, βλ. module doc.
fn read_dump_mono(dump: &str) -> Vec<f32> {
    let bytes = std::fs::read(dump).expect("read dump");
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
    inter.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect()
}

/// ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ, βλ. module doc.
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

/// ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ, βλ. module doc. ΜΟΝΗ ΠΡΟΣΘΗΚΗ: η παράμετρος
/// `split` περνάει έξω αντί να είναι πάντα η πραγματική (ο ΜΟΧΛΟΣ).
fn render(
    input: &str,
    tag: &str,
    limit: Option<f32>,
    floor: Option<f32>,
    split: Option<f32>,
) -> String {
    let out = format!("{OUT_DIR}/{tag}.wav");

    let dump = format!("/tmp/ets_src_{tag}.raw");
    let _ = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(input), &dump)
        .expect("pass0");
    let report =
        sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(&dump), false)
            .expect("trunk");
    let boundaries = report.boundaries.clone();

    let mut db = sp314_nodes::topology::DspTopologyBuilder::new("ducking_fallback_topology");
    let d_in = db.add_node("in", "Input", serde_json::json!({}));
    let d_gain = db.add_node(
        "duck_gain",
        "Gain",
        serde_json::json!({ "gain": 1.0, "glide_ms": 300.0 }),
    );
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
            pre_gain_linear: 1.0,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            max_true_peak_db: lineos_types::presets::ACX.max_true_peak_db,
            max_noise_floor_db: limit,
            input_interior_floor_db: floor,
            quiet_window_split_dbfs: split,
            input_fundamental: None,
            intent_dynamics: None,
            restoration_enabled: true,
        },
        TimelinePlan {
            boundaries,
            flagged_indices: flagged,
            pre_analysis: None,
        },
        rx_res,
    )
    .expect("render");
    let _ = std::fs::remove_file(&dump);
    out
}

fn main() {
    println!("═══ ΠΡΟΒΛΕΨΗ (πλήρες κείμενο στο doc-comment της κορυφής) ═══");
    println!("Συμφωνία: κλίση ~0.5dB/dB, monte_cristo περνάει στο +2 όχι στο -2, huckfinn αδιάφορο.");
    println!("Διαφωνία: ΟΧΙ γραμμική σε όλο το εύρος — φθίνουσα απόδοση προς τα κάτω,");
    println!("απότομη αύξηση προς τα πάνω (πιάνει ομιλία). Άγνωστο: πόσο στο +4 — μόνο κατεύθυνση.\n");

    std::fs::create_dir_all(OUT_DIR).expect("mkdir");
    let limit = lineos_types::presets::ACX.max_noise_floor_db.unwrap();
    let edge = lineos_types::presets::ACX.room_tone_max_s.unwrap();
    println!("limit(ACX max_noise_floor_db) = {limit} dBFS · edge = {edge}s\n");

    let files: [(&str, &str); 3] = [
        (
            "/home/aidevcon/Downloads/DATASET/librivox-hq/huckfinn_01_twain_apc.mp3",
            "huckfinn",
        ),
        (
            "/home/aidevcon/Downloads/DATASET/librivox-hq/count_of_monte_cristo_001_dumas.mp3",
            "monte_cristo",
        ),
        (
            "/home/aidevcon/Downloads/DATASET/librivox-hq/anne_of_green_gables_01_montgomery.mp3",
            "anne",
        ),
    ];
    println!("ΑΡΧΕΙΑ: huckfinn (φτάνει -60 με άνεση) · monte_cristo (στο ξυράφι) · anne (τρίτο, δηλωμένο — δεν φτάνει καθόλου, ελέγχεται πόσο κοντά φέρνει η σάρωση).\n");

    for (path, stem) in files {
        println!("═══════════════════════════════════════════════════════════");
        println!("{stem}");

        let src_dump = format!("/tmp/ets_scout_{stem}.raw");
        let _ = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &src_dump)
            .expect("pass0");
        let rep = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
            std::path::Path::new(&src_dump),
            false,
            edge,
        )
        .expect("trunk");
        let src_interior = rep.acx_interior_noise_floor.map(|(db, _)| db);
        let src_split = rep.quiet_window_split_dbfs;
        let _ = std::fs::remove_file(&src_dump);

        let (Some(otsu), Some(floor)) = (src_split, src_interior) else {
            println!("  ⚠ ΔΕΝ υπάρχει τομή Otsu ή interior — το αρχείο δεν μετράει γι' αυτό το τεστ.\n");
            continue;
        };
        println!("  τομή Otsu (πραγματική) = {otsu:.3} dBFS · interior = {floor:.3} dBFS · limit = {limit} dBFS");
        println!("  πύλη «τρέχει ο expander;»: {}", if floor > limit { "ΝΑΙ (floor > limit)" } else { "ΟΧΙ" });

        // ── OFF, μία φορά, αναφορά ──
        let off = render(path, &format!("{stem}__OFF"), Some(limit), Some(floor), None);
        let (mo, mono_off) = measure(&off, &format!("{stem}_off"));
        println!(
            "  OFF: interior={:.3} absolute={:.3} lufs={:.3}",
            mo.interior.unwrap_or(f32::NAN),
            mo.absolute.unwrap_or(f32::NAN),
            mo.lufs
        );

        // ── ON, πέντε τομές ──
        let mut base_windows: Option<Vec<f32>> = None; // otsu+0, αναφορά για 3.2/3.3
        let mut rows: Vec<(f32, f32, Measured)> = Vec::new();
        for off_db in OFFSETS {
            let split = otsu + off_db;
            let tag = format!("{stem}__ON_otsu{off_db:+.0}");
            let on = render(path, &tag, Some(limit), Some(floor), Some(split));
            let (mn, mono_on) = measure(&on, &format!("{stem}_on_{off_db:+.0}"));
            if off_db == 0.0 {
                base_windows = Some(rms_100ms(&mono_on, 48_000));
            }
            rows.push((off_db, split, mn));
        }

        println!("\n  ΒΗΜΑ 2 — Ο ΠΙΝΑΚΑΣ (OFF interior={:.3} absolute={:.3} lufs={:.3})",
            mo.interior.unwrap_or(f32::NAN), mo.absolute.unwrap_or(f32::NAN), mo.lufs);
        println!(
            "  {:<10}{:>10}{:>12}{:>12}{:>10}{:>10}{:>12}",
            "τομή", "split", "interior", "Δinterior", "absolute", "φτάνει-60", "ΔLUFS"
        );
        for (off_db, split, mn) in &rows {
            let interior = mn.interior.unwrap_or(f32::NAN);
            let absolute = mn.absolute.unwrap_or(f32::NAN);
            let d_interior = interior - mo.interior.unwrap_or(f32::NAN);
            let d_lufs = mn.lufs - mo.lufs;
            let reaches = absolute <= -60.0;
            println!(
                "  otsu{off_db:+>3.0}  {split:>10.3}{interior:>12.3}{d_interior:>12.4}{absolute:>10.3}{:>10}{d_lufs:>12.4}",
                if reaches { "ΝΑΙ" } else { "ΟΧΙ" }
            );
        }

        // κλίση: γραμμική παλινδρόμηση Δinterior έναντι offset (πάνω στα 5 σημεία)
        let xs: Vec<f32> = rows.iter().map(|(o, _, _)| *o).collect();
        let ys: Vec<f32> = rows
            .iter()
            .map(|(_, _, m)| m.interior.unwrap_or(f32::NAN) - mo.interior.unwrap_or(f32::NAN))
            .collect();
        let n = xs.len() as f32;
        let mean_x = xs.iter().sum::<f32>() / n;
        let mean_y = ys.iter().sum::<f32>() / n;
        let cov: f32 = xs.iter().zip(&ys).map(|(x, y)| (x - mean_x) * (y - mean_y)).sum();
        let var: f32 = xs.iter().map(|x| (x - mean_x).powi(2)).sum();
        let slope = cov / var;
        println!("\n  ΚΛΙΣΗ (γραμμική παλινδρόμηση Δinterior/offset, 5 σημεία): {slope:.4} dB/dB");
        // κλίση ανά διάστημα, όχι μόνο συνολική
        for w in rows.windows(2) {
            let (o0, _, m0v) = &w[0];
            let (o1, _, m1v) = &w[1];
            let d0 = m0v.interior.unwrap_or(f32::NAN) - mo.interior.unwrap_or(f32::NAN);
            let d1 = m1v.interior.unwrap_or(f32::NAN) - mo.interior.unwrap_or(f32::NAN);
            let local_slope = (d1 - d0) / (o1 - o0);
            println!("    otsu{o0:+.0}→otsu{o1:+.0}: τοπική κλίση {local_slope:.4} dB/dB");
        }

        // ── ΒΗΜΑ 3.2/3.3, ανά τομή έναντι otsu+0 ──
        if let Some(base) = &base_windows {
            for (off_db, _split, _mn) in &rows {
                if *off_db == 0.0 {
                    continue;
                }
                // ξαναρέντερ για να πάρουμε mono — ΔΕΝ το κρατήσαμε παραπάνω για μνήμη.
                let tag = format!("{stem}__ON_otsu{off_db:+.0}.wav");
                let path_wav = format!("{OUT_DIR}/{tag}");
                let (_mm, mono) = measure(&path_wav, &format!("{stem}_re_{off_db:+.0}"));
                let variant = rms_100ms(&mono, 48_000);
                let n = base.len().min(variant.len());

                println!("\n  ΒΗΜΑ 3.2 — otsu{off_db:+.0} έναντι otsu+0, ΚΑΤΑΝΟΜΗ (100ms)");
                let edges = [-200.0, -90.0, -80.0, -70.0, -60.0, -50.0, -40.0, 0.0];
                for w in edges.windows(2) {
                    let ca = base[..n].iter().filter(|v| **v >= w[0] && **v < w[1]).count();
                    let cb = variant[..n].iter().filter(|v| **v >= w[0] && **v < w[1]).count();
                    println!(
                        "    [{:>6.0}, {:>4.0})  otsu+0 {:>6}  otsu{off_db:+.0} {:>6}  Δ {:>+6}",
                        w[0], w[1], ca, cb, cb as i64 - ca as i64
                    );
                }

                let mut diffs: Vec<(usize, f32)> = (0..n)
                    .map(|i| (i, (variant[i] - base[i]).abs()))
                    .filter(|(_, d)| *d > 0.01)
                    .collect();
                let total_diff = diffs.len();
                diffs.sort_by(|x, y| y.1.total_cmp(&x.1));
                let loud = diffs.iter().filter(|(i, _)| base[*i] >= -40.0).count();
                println!("    παράθυρα με διαφορά: {total_diff}/{n} · πάνω από -40dBFS: {loud}");
                println!("    ΟΙ ΤΡΕΙΣ ΜΕΓΑΛΥΤΕΡΕΣ:");
                for (i, dv) in diffs.iter().take(3) {
                    println!(
                        "      {:>8.1}s  Δ {:>7.2}dB  (otsu+0 {:.1} → otsu{off_db:+.0} {:.1})",
                        *i as f32 / 10.0,
                        dv,
                        base[*i],
                        variant[*i]
                    );
                }
            }
        }
        println!();
    }
}
