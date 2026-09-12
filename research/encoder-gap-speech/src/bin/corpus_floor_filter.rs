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

    println!("{:<45} {:>6} {:>3} {:>14} {}", "αρχείο", "sr", "ch", "noise_floor_db", "ΠΕΡΝΑΕΙ (<-60)");
    let mut results = Vec::new();
    for f in &files {
        let (sr, ch) = ffprobe_sr_ch(f);
        let mono = decode_to_mono_f32(f, sr, ch);

        let mut acx = AcxCheckAnalyzer::new(sr);
        for chunk in mono.chunks(4096) {
            acx.feed_chunk(chunk);
        }
        let report = acx.finish();

        let name = Path::new(f).file_name().unwrap().to_string_lossy();
        match report.noise_floor_db {
            Some(nf) => {
                let pass = nf < -60.0;
                let window_sec = report
                    .quietest_window_start_frame
                    .map(|f| f as f64 / sr as f64)
                    .unwrap_or(f64::NAN);
                let total_sec = mono.len() as f64 / sr as f64;
                println!(
                    "{:<45} {:>6} {:>3} {:>14.2} {}  quietest_window_at={:.1}s/{:.1}s",
                    name, sr, ch, nf, if pass { "ΝΑΙ" } else { "ΟΧΙ" }, window_sec, total_sec
                );
                results.push((name.into_owned(), Some(nf), pass));
            }
            None => {
                println!("{:<45} {:>6} {:>3} {:>14} {}", name, sr, ch, "None", "ΑΠΟΝ (<1s)");
                results.push((name.into_owned(), None, false));
            }
        }
    }

    let pass_count = results.iter().filter(|(_, _, p)| *p).count();
    println!("\n=== ΣΥΝΟΨΗ AcxCheckAnalyzer ===");
    println!("< -60dBFS: {pass_count}/{}", results.len());
    println!("DONE");
}
