//! ΜΕΤΡΗΣΗ: τι λέει ΤΩΡΑ ο έλεγχος room tone, με τα ΔΙΟΡΘΩΜΕΝΑ όρια.
//!
//! Καλεί τον ΠΡΑΓΜΑΤΙΚΟ `m0d::handlers::deliver::run_deliver_core` — τη
//! συνάρτηση που περιέχει τον έλεγχο (deliver.rs:494-558) — με δύο
//! κατασκευασμένα PCM:
//!   Α. «σημερινό παραδοτέο»: κοντό head, ουρά ~1.7 s
//!   Β. «ουρά 6 s»: πάνω από το ΑΝΩ όριο της πηγής
//! Καμία επανυλοποίηση του ελέγχου· διαβάζουμε τα `warnings` που
//! επιστρέφει η ίδια η παραγωγή.
//!
//! Το PCM είναι interleaved f32 48k stereo (ό,τι περιμένει το
//! blob path), ίδιο ιδίωμα με το `generate_test_pcm` του deliver_acx.rs.

use std::io::Write;

const SR: usize = 48_000;

/// Γράφει: head_sec ησυχία → speech_sec ομιλία → tail_sec ησυχία.
/// Η «ησυχία» είναι room tone στα −70 dBFS (ΟΧΙ ψηφιακή σιωπή) ώστε να
/// μετρηθεί από την edge_quiet_secs ως ΠΑΡΟΥΣΙΑ ΗΣΥΧΙΑΣ, όπως ορίζει
/// το doc-comment της.
fn write_pcm(path: &std::path::Path, head_sec: f32, speech_sec: f32, tail_sec: f32) {
    let f = std::fs::File::create(path).unwrap();
    let mut w = std::io::BufWriter::new(f);
    let mut phase = 0.0f32;
    let total = ((head_sec + speech_sec + tail_sec) * SR as f32) as usize;
    let head_n = (head_sec * SR as f32) as usize;
    let speech_end = head_n + (speech_sec * SR as f32) as usize;

    for i in 0..total {
        let mut s = (phase * std::f32::consts::TAU).sin();
        phase += 440.0 / SR as f32;
        if phase >= 1.0 {
            phase -= 1.0;
        }
        // −70 dBFS room tone έξω από την ομιλία, −20 dBFS μέσα.
        s *= if i >= head_n && i < speech_end { 0.1 } else { 0.000316 };
        let b = s.to_le_bytes();
        w.write_all(&b).unwrap();
        w.write_all(&b).unwrap();
    }
    w.flush().unwrap();
}

fn track(id: &str, dur_ms: u64, path: &str) -> m0d::db::Track {
    m0d::db::Track {
        id: Some(id.to_string()),
        project_id: "proj_1".into(),
        track_id: id.to_string(),
        audio_path: path.into(),
        blob_id: "blob_1".into(),
        blob_path: "/tmp/blob_1.json".into(),
        lufs: -14.0,
        true_peak: -1.0,
        flavour_id: "test".into(),
        created_at: "now".into(),
        duration_ms: dur_ms,
    }
}

fn run(tag: &str, head: f32, speech: f32, tail: f32) {
    let dir = std::env::temp_dir().join(format!("spacing_verdict_{tag}"));
    std::fs::create_dir_all(&dir).unwrap();
    let pcm = dir.join("in.pcm");
    write_pcm(&pcm, head, speech, tail);

    let dur_ms = ((head + speech + tail) * 1000.0) as u64;
    let tracks = vec![track("t1", dur_ms, pcm.to_str().unwrap())];

    let req = m0d::handlers::deliver::DeliverRequest {
        output_dir: Some(dir.join("out").to_string_lossy().into_owned()),
        book_title: "Spacing Probe".into(),
        entries: vec![m0d::handlers::deliver::DeliverEntry {
            track_id: "t1".into(),
            index: 1,
            title: "Chap 1".into(),
            role: "chapter".into(),
        }],
    };

    let plan = m0d::handlers::deliver::validate_and_plan(&req, &tracks).unwrap();
    let resp = m0d::handlers::deliver::run_deliver_core(&req, plan, None, None).unwrap();

    println!("── {tag}  (head {head}s · ομιλία {speech}s · tail {tail}s)");
    let spacing: Vec<&String> = resp
        .warnings
        .iter()
        .filter(|w| w.contains("room tone"))
        .collect();
    if spacing.is_empty() {
        println!("     ΚΑΜΙΑ προειδοποίηση room tone");
    } else {
        for w in spacing {
            println!("     {w}");
        }
    }
    for w in resp.warnings.iter().filter(|w| !w.contains("room tone")) {
        println!("     (άλλο) {w}");
    }
    println!();
}

fn main() {
    println!("ΟΡΓΑΝΟ: m0d::handlers::deliver::run_deliver_core (ΠΡΑΓΜΑΤΙΚΗ)\n");
    // Α — το σχήμα του σημερινού παραδοτέου: head κάτω από τη σύσταση,
    //     tail μέσα στο [1,5].
    run("A_simerino", 0.1, 6.0, 1.7);
    // Β — ουρά πάνω από το ΑΝΩ όριο («must not exceed 5 seconds»).
    run("B_tail_6s", 1.5, 6.0, 6.0);
}
