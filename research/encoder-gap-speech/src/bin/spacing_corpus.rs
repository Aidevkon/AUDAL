//! ΜΕΤΡΗΣΗ: το spacing ΠΡΑΓΜΑΤΙΚΟΥ ΕΚΔΟΜΕΝΟΥ υλικού.
//!
//! Καλεί την **ΠΡΑΓΜΑΤΙΚΗ** `m0d::handlers::export::edge_quiet_secs`
//! (export.rs:544 — pub) — ό,τι ακριβώς γεμίζει τα
//! `AcxExportOutcome::{head_quiet_secs, tail_quiet_secs}`. Καμία
//! επανυλοποίηση του κατωφλίου (−50 dBFS) ή του παραθύρου (100 ms).
//!
//! ⚠ ΩΜΟ ΑΡΧΕΙΟ, ΟΧΙ ΚΛΙΠ: κάθε αρχείο αποκωδικοποιείται ΟΛΟΚΛΗΡΟ.
//! Downmix (l+r)*0.5 στον ΕΓΓΕΝΗ ρυθμό — ίδιο μοτίβο με το βήμα 3 της
//! export_mp3_acx, χωρίς resample (τα αρχεία είναι ήδη 44.1k, δηλαδή
//! ο ίδιος ο target_sr — άρα κανένα στάδιο δεν παρακάμπτεται σιωπηλά).

const ACX_HEAD_MIN: f32 = 0.5;
const ACX_HEAD_MAX: f32 = 1.0;
const ACX_TAIL_MIN: f32 = 1.0;
const ACX_TAIL_MAX: f32 = 5.0;

fn decode_mono(src: &str, sr: u32) -> Option<Vec<f32>> {
    let dump = std::env::temp_dir().join(format!("spacing_probe_{}.pcm", std::process::id()));
    let ok = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i", src, "-ar", &sr.to_string(),
               "-ac", "1", "-f", "f32le"])
        .arg(&dump)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return None;
    }
    let bytes = std::fs::read(&dump).ok()?;
    let _ = std::fs::remove_file(&dump);
    Some(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

fn stats(v: &mut Vec<f32>) -> (f32, f32, f32) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    let median = if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) * 0.5
    };
    (median, v[0], v[n - 1])
}

fn main() {
    let sr: u32 = 44_100;
    let files: Vec<String> = std::env::args().skip(1).collect();
    assert!(!files.is_empty(), "usage: spacing_corpus <file>...");

    println!("ΟΡΓΑΝΟ: m0d::handlers::export::edge_quiet_secs (ΠΡΑΓΜΑΤΙΚΗ)");
    println!("κατώφλι −50 dBFS · παράθυρο 100 ms · ΩΜΟ αρχείο, mono @ {sr} Hz\n");
    println!("{:<44} {:>8} {:>8}   {:>5} {:>5}", "αρχείο", "head(s)", "tail(s)", "HEAD", "TAIL");

    let mut heads = Vec::new();
    let mut tails = Vec::new();
    let mut head_ok = 0usize;
    let mut tail_ok = 0usize;

    for f in &files {
        let Some(mono) = decode_mono(f, sr) else {
            println!("{:<44} DECODE FAILED", short(f));
            continue;
        };
        let (head, tail) = m0d::handlers::export::edge_quiet_secs(&mono, sr);

        let h_in = (ACX_HEAD_MIN..=ACX_HEAD_MAX).contains(&head);
        let t_in = (ACX_TAIL_MIN..=ACX_TAIL_MAX).contains(&tail);
        if h_in { head_ok += 1; }
        if t_in { tail_ok += 1; }
        heads.push(head);
        tails.push(tail);

        println!(
            "{:<44} {:>8.2} {:>8.2}   {:>5} {:>5}",
            short(f), head, tail,
            if h_in { "εντός" } else { "ΕΚΤΟΣ" },
            if t_in { "εντός" } else { "ΕΚΤΟΣ" },
        );
    }

    let n = heads.len();
    let (hm, hmin, hmax) = stats(&mut heads);
    let (tm, tmin, tmax) = stats(&mut tails);

    println!("\n═══ ΣΥΝΟΨΗ (n={n}) ═══");
    println!("  head εντός [{ACX_HEAD_MIN}, {ACX_HEAD_MAX}] : {head_ok}/{n}");
    println!("  tail εντός [{ACX_TAIL_MIN}, {ACX_TAIL_MAX}] : {tail_ok}/{n}");
    println!("  head  median {hm:.2}  min {hmin:.2}  max {hmax:.2}   (spread {:.2})", hmax - hmin);
    println!("  tail  median {tm:.2}  min {tmin:.2}  max {tmax:.2}   (spread {:.2})", tmax - tmin);
}

fn short(p: &str) -> String {
    let b = std::path::Path::new(p)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if b.chars().count() > 42 {
        b.chars().take(42).collect()
    } else {
        b
    }
}
