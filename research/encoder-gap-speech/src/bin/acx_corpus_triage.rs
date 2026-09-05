//! ΑΠΟΓΡΑΦΗ + ΔΙΑΛΟΓΗ του corpus `~/Downloads/DATASET/acx/`.
//!
//! ΟΛΑ τα μεγέθη από ΥΠΑΡΧΟΝΤΑ όργανα:
//!   · `sp314_dsp::analysis::stereo::stereo_correlation`
//!   · `sp314_dsp::analysis::dynamics::crest_factor_db`
//!   · `sp314_dsp::analysis::acx_check::AcxCheckAnalyzer`  (rms, peak, noise_floor)
//!   · `m0d::handlers::export::edge_quiet_secs`            (head/tail spacing)
//! Το `active_frac` είναι το ΙΔΙΟ κατώφλι/παράθυρο με το
//! `rms_lufs_distance.rs` (100 ms sub-blocks, −50 dBFS) — ίδιος ορισμός,
//! ώστε τα νούμερα να συγκρίνονται με το F-086.
//!
//! ⚠ ΚΑΝΕΝΑ ΦΑΣΜΑΤΙΚΟ ΠΡΟΦΙΛ: θα ήταν κυκλικό να φιλτράρουμε το corpus
//! με βάση το μέγεθος που το corpus υπάρχει για να βαθμονομήσει.
//!
//! ΚΑΙ ΤΟ ΚΑΘΑΡΟΤΕΡΟ ΣΗΜΑ (ΜΕΤΡΗΜΕΝΟ 25/08, τέλειος διαχωρισμός στα
//! AUDIOBFLAC): `RMS(0-0.5s)`. `−inf` ⇒ ψηφιακή σιωπή στην αρχή·
//! πλήρες σήμα ⇒ μηδέν room tone.

use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;

const SR: u32 = 44_100;
const ACTIVE_THRESH_DB: f32 = -50.0;

fn decode(src: &str, ch: u16) -> Option<Vec<f32>> {
    let dump = std::env::temp_dir().join(format!("triage_{}_{}.pcm", std::process::id(), ch));
    let ok = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i", src, "-ar", &SR.to_string(),
               "-ac", &ch.to_string(), "-f", "f32le"])
        .arg(&dump).status().map(|s| s.success()).unwrap_or(false);
    if !ok { return None; }
    let b = std::fs::read(&dump).ok()?;
    let _ = std::fs::remove_file(&dump);
    Some(b.chunks_exact(4).map(|x| f32::from_le_bytes([x[0],x[1],x[2],x[3]])).collect())
}

/// Κλάσμα των 100 ms υπο-μπλοκ πάνω από −50 dBFS. ΙΔΙΟΣ ορισμός με το
/// rms_lufs_distance.rs (F-086) — όχι νέος.
fn active_frac(mono: &[f32]) -> f32 {
    let n = (SR / 10) as usize;
    let (mut total, mut active) = (0usize, 0usize);
    for w in mono.chunks(n) {
        if w.len() < n { break; }
        let ss: f64 = w.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (ss / n as f64).sqrt();
        let db = if rms > 1e-10 { 20.0 * rms.log10() } else { -200.0 };
        total += 1;
        if db > ACTIVE_THRESH_DB as f64 { active += 1; }
    }
    if total == 0 { 0.0 } else { active as f32 / total as f32 }
}

fn rms_db(sl: &[f32]) -> f32 {
    if sl.is_empty() { return f32::NEG_INFINITY; }
    let ss: f64 = sl.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let r = (ss / sl.len() as f64).sqrt();
    if r > 1e-12 { 20.0 * (r as f32).log10() } else { f32::NEG_INFINITY }
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    println!("file\tdur_s\tcorr\tcrest_db\tactive_frac\tnoise_floor\trms\thead\ttail\trms_first_0_5s");
    for f in &files {
        let Some(st) = decode(f, 2) else { continue };
        let Some(mono) = decode(f, 1) else { continue };
        if mono.is_empty() { continue; }

        let l: Vec<f32> = st.iter().step_by(2).copied().collect();
        let r: Vec<f32> = st.iter().skip(1).step_by(2).copied().collect();
        let corr = sp314_dsp::analysis::stereo::stereo_correlation(&l, &r);
        let crest = sp314_dsp::analysis::dynamics::crest_factor_db(&mono);

        let mut a = AcxCheckAnalyzer::new(SR);
        for c in mono.chunks(4096) { a.feed_chunk(c); }
        let rep = a.finish();

        let (head, tail) = m0d::handlers::export::edge_quiet_secs(&mono, SR);
        let first = rms_db(&mono[..mono.len().min((SR / 2) as usize)]);

        let name = std::path::Path::new(f).file_name().unwrap().to_string_lossy();
        println!(
            "{}\t{:.1}\t{:.4}\t{:.2}\t{:.4}\t{}\t{:.2}\t{:.2}\t{:.2}\t{}",
            name,
            mono.len() as f32 / SR as f32,
            corr,
            crest,
            active_frac(&mono),
            rep.noise_floor_db.map(|v| format!("{v:.2}")).unwrap_or_else(|| "None".into()),
            rep.rms_db,
            head,
            tail,
            if first.is_finite() { format!("{first:.2}") } else { "-inf".into() },
        );
    }
}
