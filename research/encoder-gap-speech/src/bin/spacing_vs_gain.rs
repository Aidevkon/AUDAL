//! ΜΕΤΡΗΣΗ: επηρεάζει ΟΜΟΙΟΜΟΡΦΟ gain το edge_quiet_secs;
//!
//! Η RMS correction της export_mp3_acx (:742-752) και το peak trim
//! (:783-791) είναι **στατικά ομοιόμορφα gain** πάνω στο ίδιο `mono`
//! buffer, και το `edge_quiet_secs` (:971) το μετράει ΜΕΤΑ. Το κατώφλι
//! του είναι **ΑΠΟΛΥΤΟ** (−50 dBFS), όχι σχετικό — άρα η ερώτηση
//! ανάγεται ακριβώς σε: «τι κάνει ένα ομοιόμορφο gain σε ένα απόλυτο
//! κατώφλι;» και απαντιέται χωρίς αναπαραγωγή resample/LAME.
//!
//! Καλεί την ΠΡΑΓΜΑΤΙΚΗ `m0d::handlers::export::edge_quiet_secs`.
//! Τυπώνει και RMS/noise-floor μέσω του ΠΡΑΓΜΑΤΙΚΟΥ `AcxCheckAnalyzer`.

use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;

fn decode_mono(src: &str, sr: u32) -> Option<Vec<f32>> {
    let dump = std::env::temp_dir().join(format!("svg_probe_{}.pcm", std::process::id()));
    let ok = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i", src, "-ar", &sr.to_string(),
               "-ac", "1", "-f", "f32le"])
        .arg(&dump)
        .status().map(|s| s.success()).unwrap_or(false);
    if !ok { return None; }
    let bytes = std::fs::read(&dump).ok()?;
    let _ = std::fs::remove_file(&dump);
    Some(bytes.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

fn measure(mono: &[f32], sr: u32) -> (f32, f32, Option<f32>, f32, f32) {
    let mut a = AcxCheckAnalyzer::new(sr);
    for c in mono.chunks(4096) { a.feed_chunk(c); }
    let r = a.finish();
    let (h, t) = m0d::handlers::export::edge_quiet_secs(mono, sr);
    (r.rms_db, r.sample_peak_db, r.noise_floor_db, h, t)
}

fn main() {
    let sr = 44_100u32;
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: spacing_vs_gain <file> [gain_db ...]");
    let gains: Vec<f32> = args.filter_map(|s| s.parse().ok()).collect();

    let base = decode_mono(&path, sr).expect("decode");
    let name = std::path::Path::new(&path).file_stem().unwrap().to_string_lossy();

    println!("ΑΡΧΕΙΟ: {name}   ({} frames @ {sr} Hz mono)", base.len());
    let (rms0, pk0, nf0, h0, t0) = measure(&base, sr);
    println!("  ΩΜΟ:  rms {rms0:>8.3}  peak {pk0:>8.3}  noise_floor {:>9}  head {h0:>5.2}  tail {t0:>5.2}",
             nf0.map(|v| format!("{v:.3}")).unwrap_or_else(|| "None".into()));

    if gains.is_empty() { return; }
    println!("\n  gain_db     rms_db    peak_db  noise_floor    head    tail   Δhead");
    for g in gains {
        let lin = libm::powf(10.0, g / 20.0);
        let shifted: Vec<f32> = base.iter().map(|s| s * lin).collect();
        let (rms, pk, nf, h, t) = measure(&shifted, sr);
        println!("  {g:>+7.3}  {rms:>9.3}  {pk:>9.3}  {:>11}  {h:>6.2}  {t:>6.2}  {:>+6.2}",
                 nf.map(|v| format!("{v:.3}")).unwrap_or_else(|| "None".into()),
                 h - h0);
    }
}
