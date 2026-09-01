//! ΜΕΤΡΗΣΗ: παλεύει το LTASS με το low-cut;
//!
//! Τρέχει το ΙΔΙΟ όργανο (run_trunk_pass → ReferenceResolver) ΔΥΟ φορές:
//!   A. Α-PATH: raw signal → trunk_pass → gains  (σημερινή συμπεριφορά)
//!   Β. Β-PATH: lowcut 80 Hz HP πρώτα → trunk_pass → gains
//!
//! ΠΡΟΒΛΕΨΗ: στο B, bands 0-1 (20-80 / 80-250 Hz) αλλάζουν δραστικά.
//!
//! Χρησιμοποιεί το ΠΡΑΓΜΑΤΙΚΟ Biquad::new(FilterType::HighPass, 80.0, 0.707)
//! — ακριβώς ό,τι έχει η RestorationChain [cleaner.rs:69].
//! Καμία αλλαγή παραγωγής.

use sp314_dsp::restoration::biquad::{Biquad, FilterType};
use std::io::Write;

const CENTERS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];
const BAND_NAMES: [&str; 8] = [
    "B0 20-80Hz  ",
    "B1 80-250Hz ",
    "B2 250-500Hz",
    "B3 500-1kHz ",
    "B4 1-2kHz   ",
    "B5 2-4kHz   ",
    "B6 4-8kHz   ",
    "B7 8-20kHz  ",
];

fn read_wav(path: &str) -> (Vec<f32>, Vec<f32>, u32) {
    let mut r = hound::WavReader::open(path).expect("open wav");
    let spec = r.spec();
    let ch = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.unwrap() as f32 / max).collect()
        }
    };
    let mut l = Vec::with_capacity(samples.len() / ch);
    let mut rt = Vec::with_capacity(samples.len() / ch);
    for frame in samples.chunks(ch) {
        l.push(frame[0]);
        rt.push(if ch >= 2 { frame[1] } else { frame[0] });
    }
    (l, rt, spec.sample_rate)
}

/// Γράφει interleaved f32 PCM dump, επιστρέφει path.
fn write_pcm_dump(l: &[f32], r: &[f32], tag: &str) -> std::path::PathBuf {
    let dump = std::env::temp_dir().join(format!(
        "ltass_conflict_{}_{}.pcm",
        tag,
        std::process::id()
    ));
    let f = std::fs::File::create(&dump).expect("create dump");
    let mut w = std::io::BufWriter::new(f);
    for i in 0..l.len() {
        w.write_all(&l[i].to_ne_bytes()).unwrap();
        w.write_all(&r[i].to_ne_bytes()).unwrap();
    }
    drop(w);
    dump
}

/// Εφαρμόζει low-cut (HP Butterworth Q=0.707 @ 80 Hz) — ό,τι κάνει η
/// RestorationChain [cleaner.rs:69-90].
fn apply_lowcut(l: &[f32], r: &[f32], sr: u32) -> (Vec<f32>, Vec<f32>) {
    let mut hp = Biquad::new(FilterType::HighPass, 80.0, 0.707, sr as f32);
    let mut out_l = Vec::with_capacity(l.len());
    let mut out_r = Vec::with_capacity(r.len());
    for i in 0..l.len() {
        let (ol, or_) = hp.process_stereo(l[i], r[i]);
        out_l.push(ol);
        out_r.push(or_);
    }
    (out_l, out_r)
}

/// Εκτελεί trunk_pass και επιστρέφει (spectral_profile_db_normalized[8], gains[8]).
fn measure_gains(l: &[f32], r: &[f32]) -> ([f32; 8], [f32; 8]) {
    let dump = write_pcm_dump(l, r, "tmp");
    let trunk = sp314_orchestrator::trunk_pass::run_trunk_pass(&dump, false)
        .expect("trunk pass");
    let _ = std::fs::remove_file(&dump);

    let prof = aether_bridge::reference_resolver::ReferenceProfile::load(
        aether_bridge::reference_resolver::ProfileId::PodcastV1,
    );
    let n = prof.normalization_band_count;
    let raw = trunk.spectral_profile_db;
    let mean: f32 = raw[..n].iter().sum::<f32>() / n as f32;
    let normalized: [f32; 8] = std::array::from_fn(|k| raw[k] - mean);
    let gains = aether_bridge::reference_resolver::ReferenceResolver::resolve(&normalized, &prof);
    (normalized, gains)
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: ltass_lowcut_conflict <wav>");
    let (l, r, sr) = read_wav(&path);
    println!(
        "MEASURED input: {} frames @ {} Hz  [{}]",
        l.len(),
        sr,
        std::path::Path::new(&path)
            .file_name()
            .unwrap()
            .to_string_lossy()
    );

    // ── Α-PATH: raw signal ──────────────────────────────────────────────────
    println!("\n=== A-PATH: raw → trunk_pass → gains (σημερινή συμπεριφορά) ===");
    let (norm_a, gains_a) = measure_gains(&l, &r);

    // ── Β-PATH: lowcut πρώτα ─────────────────────────────────────────────────
    println!("=== B-PATH: lowcut 80Hz HP (Q=0.707) → trunk_pass → gains ===");
    let (l_lc, r_lc) = apply_lowcut(&l, &r, sr);
    let (norm_b, gains_b) = measure_gains(&l_lc, &r_lc);

    // ── ΠΑΡΑΛΛΗΛΗ ΕΚΘΕΣΗ ────────────────────────────────────────────────────
    const G_MAX: f32 = 6.0;
    let clamp_str = |g: f32| -> String {
        let clamped = (g.abs() - G_MAX).abs() < 1e-3;
        if clamped { "★CLAMP".to_string() } else { "      ".to_string() }
    };

    println!();
    println!(
        "  {:12}  {:>9} {:>9} {:>9}   {:>9} {:>9} {:>9}",
        "band",
        "normA(dB)",
        "gainA(dB)",
        "★clamp?",
        "normB(dB)",
        "gainB(dB)",
        "★clamp?"
    );
    println!("  {:-<12}  {:-<9} {:-<9} {:-<9}   {:-<9} {:-<9} {:-<9}", "", "", "", "", "", "", "");

    for k in 0..8 {
        let ga = gains_a[k];
        let gb = gains_b[k];
        let delta_norm = norm_b[k] - norm_a[k];
        let delta_gain = gb - ga;
        let significant = delta_gain.abs() > 0.05;
        let marker = if significant { " ◄" } else { "" };
        println!(
            "  {:12}  {:>9.3} {:>9.3} {:>9}   {:>9.3} {:>9.3} {:>9}  Δgain={:+.3}{}",
            BAND_NAMES[k],
            norm_a[k],
            ga,
            clamp_str(ga),
            norm_b[k],
            gb,
            clamp_str(gb),
            delta_gain,
            marker
        );
        let _ = delta_norm; // used in Δgain display indirectly
    }

    // ── ΣΥΝΟΨΗ ──────────────────────────────────────────────────────────────
    println!();
    let bands_changed: Vec<usize> = (0..8)
        .filter(|&k| (gains_b[k] - gains_a[k]).abs() > 0.05)
        .collect();
    let b01_delta: [f32; 2] = [gains_b[0] - gains_a[0], gains_b[1] - gains_a[1]];
    println!("=== ΣΥΝΟΨΗ ===");
    println!(
        "  Ζώνες που αλλάζουν >0.05 dB: {:?}",
        bands_changed
            .iter()
            .map(|&k| format!("[{}] {} Δ{:+.3}", k, CENTERS[k], gains_b[k] - gains_a[k]))
            .collect::<Vec<_>>()
    );
    println!("  Band 0 (50Hz)  Δgain = {:+.3} dB", b01_delta[0]);
    println!("  Band 1 (150Hz) Δgain = {:+.3} dB", b01_delta[1]);

    let b0_drops = b01_delta[0] < -0.5;
    let b1_drops = b01_delta[1] < -0.5;
    let b0_no_clamp_b = gains_b[0].abs() < G_MAX - 0.01;
    let b1_no_clamp_b = gains_b[1].abs() < G_MAX - 0.01;

    println!();
    println!("=== ΑΠΟΤΕΛΕΣΜΑ ΠΡΟΒΛΕΨΗΣ ===");
    println!(
        "  ΠΡΟΒΛΕΨΗ: bands 0-1 πέφτουν δραστικά ΚΑΙ δεν κολλάνε στο clamp στο B-PATH"
    );
    println!(
        "  Band 0: Δ{:+.3} dB | no-clamp-B={}  → {}",
        b01_delta[0],
        b0_no_clamp_b,
        if b0_drops && b0_no_clamp_b { "ΝΑΙ — σύγκρουση επιβεβαιωμένη" } else if !b0_drops { "ΟΧΙ — υλικό δεν έχει σημαντική ενέργεια <80 Hz" } else { "ΜΕΡΙΚΗ" }
    );
    println!(
        "  Band 1: Δ{:+.3} dB | no-clamp-B={}  → {}",
        b01_delta[1],
        b1_no_clamp_b,
        if b1_drops && b1_no_clamp_b { "ΝΑΙ — σύγκρουση επιβεβαιωμένη" } else if !b1_drops { "ΟΧΙ — υλικό δεν έχει σημαντική ενέργεια 80-250 Hz" } else { "ΜΕΡΙΚΗ" }
    );

    let confirmed_conflict = (b0_drops || b1_drops) && (b0_no_clamp_b || b1_no_clamp_b);
    println!();
    println!(
        "  ΤΕΛΙΚΟ: {}",
        if confirmed_conflict {
            "ΕΠΙΒΕΒΑΙΩΜΕΝΗ ΣΥΓΚΡΟΥΣΗ ΣΤΑΔΙΩΝ — το LTASS μετράει ό,τι ο low-cut αλλάζει"
        } else {
            "ΧΩΡΙΣ ΣΥΓΚΡΟΥΣΗ — τα δεδομένα δεν υποστηρίζουν τη σύγκρουση σε αυτό το υλικό"
        }
    );

    println!("\nDONE");
}
