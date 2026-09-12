// ΜΕΤΡΗΘΗΚΕ 2026-09-12, 20 αρχεία LibriVox:
// Ο AcxCheckAnalyzer δίνει το ΑΠΟΛΥΤΟ ΕΛΑΧΙΣΤΟ παράθυρο
// 500 ms σε ΟΛΟ το αρχείο. Σε 11 από τα 15 «περνάει», το
// ελάχιστο βρισκόταν στα πρώτα ή τελευταία 5 δευτερόλεπτα
// — δηλαδή padding, όχι δωμάτιο. Interior-only: 4/20,
// ταυτόσημο με ανεξάρτητο p5 percentile.
// Τρία αρχεία έδωσαν ΑΚΡΙΒΩΣ −144.00 dB = ψηφιακή σιωπή
// (linear < 1e-10), όχι ησυχία — και το ACX ΑΠΑΙΤΕΙ room
// tone στα άκρα, όχι μηδενικά.
// ⇒ ΓΙΑ CORPUS GROUND TRUTH ΜΕΤΡΑΕΙ ΜΟΝΟ ΤΟ INTERIOR.
//   Το verdict του analyzer απαντά άλλη ερώτηση — τη
//   σωστή για το ACX, τη λάθος για εμάς.
// ⇒ ΤΟ ΚΡΙΤΗΡΙΟ ΔΕΝ ΕΙΝΑΙ ΤΟ ΑΠΟΛΥΤΟ FLOOR ΑΛΛΑ Η
//   ΑΠΟΣΤΑΣΗ rms − floor. Το gain μετακινεί peak, rms και
//   floor ΜΑΖΙ — απόσταση < 37 dB δεν σώζεται με gain,
//   θέλει gate ή denoise (δηλ. είναι ήδη φθαρμένο υλικό,
//   όχι ground truth).

//! RECON: noise floor των 20 LibriVox δειγμάτων με τον ΠΡΑΓΜΑΤΙΚΟ
//! sp314_dsp::analysis::acx_check::AcxCheckAnalyzer — καμία re-implementation.
//! Πρότυπο χρήσης: mp3_acx_mechanism_confirm.rs (AcxCheckAnalyzer::new →
//! feed_chunk(mono f32) σε chunks(4096) → finish()).
//!
//! Η μόνη πηγή είναι το ίδιο το mp3 (lossy) — ffmpeg το αποκωδικοποιεί σε
//! raw f32le PCM στο ΦΥΣΙΚΟ sample rate/κανάλια του αρχείου (μετρημένο από
//! ffprobe, όχι υποτιθέμενο 44100/stereo). Stereo -> mono downmix (l+r)*0.5,
//! ίδιο μοτίβο με mp3_acx_mechanism_confirm.rs.

use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use std::path::Path;
use std::process::Command;

fn ffprobe_sr_ch(path: &str) -> (u32, u32) {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error", "-select_streams", "a:0",
            "-show_entries", "stream=sample_rate,channels",
            "-of", "csv=p=0", path,
        ])
        .output()
        .expect("ffprobe");
    let s = String::from_utf8_lossy(&out.stdout);
    let mut parts = s.trim().split(',');
    let sr: u32 = parts.next().unwrap().parse().unwrap();
    let ch: u32 = parts.next().unwrap().parse().unwrap();
    (sr, ch)
}

fn decode_to_mono_f32(path: &str, sr: u32, ch: u32) -> Vec<f32> {
    let dump = std::env::temp_dir().join(format!(
        "librivox_floor_{}.pcm",
        Path::new(path).file_stem().unwrap().to_string_lossy()
    ));
    let status = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", path,
            "-ar", &sr.to_string(),
            "-ac", &ch.to_string(),
            "-f", "f32le",
        ])
        .arg(&dump)
        .status()
        .expect("ffmpeg decode");
    assert!(status.success(), "ffmpeg decode failed for {path}");

    let raw = std::fs::read(&dump).expect("read decoded pcm");
    let samples: Vec<f32> = raw
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    let _ = std::fs::remove_file(&dump);

    if ch == 1 {
        samples
    } else {
        // Verbatim downmix pattern: (l+r)*0.5, όπως mp3_acx_mechanism_confirm.rs
        samples
            .chunks_exact(ch as usize)
            .map(|frame| (frame[0] + frame[1]) * 0.5)
            .collect()
    }
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/home/aidevcon/Downloads/DATASET/librivox-hq".to_string());

    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "mp3").unwrap_or(false))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    files.sort();

    // ΤΑ ΔΥΟ ΝΟΥΜΕΡΑ ΕΡΧΟΝΤΑΙ ΑΠΟ ΤΗΝ ΠΡΟΔΙΑΓΡΑΦΗ, ΟΧΙ ΑΠΟ ΕΔΩ.
    // Και τα δύο φέρουν SOURCE + RETRIEVED στο presets.rs (:95-99, :85).
    //
    // edge_s = room_tone_max_s: ο οίκος λέει «room tone spacing must not
    // exceed 5 seconds» — ΑΝΩ ΦΡΑΓΜΑ του τι ΕΠΙΤΡΕΠΕΤΑΙ να είναι padding.
    // ⚠ Η χρήση του ως ΣΤΑΘΕΡΟΣ ΑΠΟΚΛΕΙΣΜΟΣ είναι ΕΠΙΛΟΓΗ, συντηρητική:
    //   σε αρχείο με 1 s padding, 4 s πραγματικού περιεχομένου σε κάθε άκρο
    //   δεν μετριούνται. Το κόστος είναι δηλωμένο, όχι μηδενικό.
    // limit = max_noise_floor_db: «noise floor no higher than -60dB RMS».
    //   ΚΑΝΕΙ ΚΑΙ ΤΙΣ ΔΥΟ ΔΟΥΛΕΙΕΣ — κρίση συμμόρφωσης ΚΑΙ διαχωρισμός
    //   SLEEPING/WAKING. Μηδέν νέο κατώφλι.
    let edge_s = lineos_types::presets::ACX
        .room_tone_max_s
        .expect("ACX ορίζει room_tone_max_s");
    let limit_db = lineos_types::presets::ACX
        .max_noise_floor_db
        .expect("ACX ορίζει max_noise_floor_db");
    println!("edge_s = {edge_s} (presets.rs room_tone_max_s) · limit = {limit_db} dB (max_noise_floor_db)\n");

    println!(
        "{:<40} {:>10} {:>9} {:>10} {:>9}  {}",
        "αρχείο", "abs_min", "@sec", "interior", "@sec", "ΚΑΤΑΣΤΑΣΗ"
    );
    let mut results = Vec::new();
    for f in &files {
        let (sr, ch) = ffprobe_sr_ch(f);
        let mono = decode_to_mono_f32(f, sr, ch);

        let mut acx = AcxCheckAnalyzer::new(sr);
        for chunk in mono.chunks(4096) {
            acx.feed_chunk(chunk);
        }
        // ΠΡΙΝ το finish() — το interior_noise_floor_db παίρνει &self,
        // το finish() καταναλώνει.
        let interior = acx.interior_noise_floor_db(edge_s);
        let report = acx.finish();

        let name = Path::new(f).file_name().unwrap().to_string_lossy();
        let total_sec = mono.len() as f64 / sr as f64;

        let abs_txt = match report.noise_floor_db {
            Some(nf) => format!("{nf:10.2}"),
            None => format!("{:>10}", "None"),
        };
        let abs_at = report
            .quietest_window_start_frame
            .map(|fr| format!("{:9.1}", fr as f64 / sr as f64))
            .unwrap_or_else(|| format!("{:>9}", "-"));

        // Ο ΚΑΝΟΝΑΣ, ΑΠΟ ΚΩΔΙΚΑ:
        //   δεν υπάρχει interior            ⇒ ABSENT
        //   interior κάτω από το όριο       ⇒ SLEEPING (ο gate δεν έχει δουλειά)
        //   interior πάνω από το όριο       ⇒ WAKING
        let (int_txt, int_at, state) = match interior {
            None => (
                format!("{:>10}", "None"),
                format!("{:>9}", "-"),
                "ABSENT",
            ),
            Some((db, fr)) => (
                format!("{db:10.2}"),
                format!("{:9.1}", fr as f64 / sr as f64),
                if db < limit_db { "SLEEPING" } else { "WAKING" },
            ),
        };

        println!(
            "{:<40} {} {} {} {}  {}   ({:.0}s)",
            name, abs_txt, abs_at, int_txt, int_at, state, total_sec
        );
        results.push((
            name.into_owned(),
            report.noise_floor_db,
            interior.map(|(db, _)| db),
            state,
        ));
    }

    let n = results.len();
    let sleeping = results.iter().filter(|r| r.3 == "SLEEPING").count();
    let waking = results.iter().filter(|r| r.3 == "WAKING").count();
    let absent = results.iter().filter(|r| r.3 == "ABSENT").count();
    let abs_pass = results
        .iter()
        .filter(|r| r.1.map(|v| v < limit_db).unwrap_or(false))
        .count();

    println!("\n=== ΣΥΝΟΨΗ ===");
    println!("ΑΠΟΛΥΤΟ ελάχιστο  < {limit_db} dB : {abs_pass}/{n}");
    println!("INTERIOR ελάχιστο < {limit_db} dB : {sleeping}/{n}   ← SLEEPING");
    println!("                  >= {limit_db} dB : {waking}/{n}   ← WAKING");
    println!("                  ΑΠΟΝ            : {absent}/{n}   ← ABSENT");
    println!("DONE");
}
