//! ΜΕΤΡΗΣΗ 2026-08-24 — κάνει το AutoLevel το apsolute DeEsser threshold
//! de facto σχετικό; READ+RUN, standalone research binary. ΔΕΝ αγγίζει
//! production code — καλεί τα ΠΡΑΓΜΑΤΙΚΑ DspNode impls (sp314-nodes),
//! όχι re-implementation.
//!
//! Follow-up του F-081/deess-threshold: εκείνες οι μετρήσεις έτρεξαν τον
//! DeEsser ΑΠΟΜΟΝΩΜΕΝΟ. Στην πραγματική POXVoice αλυσίδα (flavor.rs:
//! 159-177) o DeEsser έρχεται ΜΕΤΑ το AutoLevel. Εδώ: (Α) AutoLevel
//! μόνο του, RMS εξόδου στα τρία αρχεία· (Β) AutoLevel→DeEsser σε
//! σειρά, % ενεργοποίησης στα ίδια 4 thresholds.

use sp314_nodes::node::DspNode;
use sp314_nodes::nodes::autolevel::AutoLevelNode;
use sp314_nodes::nodes::deesser::DeEsserNode;
use std::path::PathBuf;

const ACT_CHUNK_MS: f64 = 10.0;
const ACT_DBFS_THRESHOLD: f64 = -60.0;

fn decode_stereo_native(path: &str) -> (Vec<f32>, Vec<f32>, u32) {
    let probe = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=sample_rate", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .expect("spawn ffprobe");
    let sr: u32 = String::from_utf8_lossy(&probe.stdout)
        .lines()
        .next()
        .expect("no sr")
        .trim()
        .parse()
        .expect("bad sr");

    let tmp = PathBuf::from(format!("/tmp/autolevel_chain_{}.pcm", std::process::id()));
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(path)
        .args(["-ac", "2", "-ar", &sr.to_string(), "-f", "f32le"])
        .arg(&tmp)
        .status()
        .expect("spawn ffmpeg");
    assert!(status.success(), "ffmpeg decode failed");
    let bytes = std::fs::read(&tmp).expect("read pcm");
    let _ = std::fs::remove_file(&tmp);
    let interleaved: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    let mut l = Vec::with_capacity(interleaved.len() / 2);
    let mut r = Vec::with_capacity(interleaved.len() / 2);
    for pair in interleaved.chunks_exact(2) {
        l.push(pair[0]);
        r.push(pair[1]);
    }
    (l, r, sr)
}

fn rms_db(v: &[f32]) -> f64 {
    if v.is_empty() {
        return f64::NEG_INFINITY;
    }
    let sum_sq: f64 = v.iter().map(|&x| (x as f64) * (x as f64)).sum();
    let rms = (sum_sq / v.len() as f64).sqrt();
    if rms < 1e-12 {
        -240.0
    } else {
        20.0 * rms.log10()
    }
}

fn combined_rms_db(l: &[f32], r: &[f32]) -> f64 {
    let mut v = Vec::with_capacity(l.len() * 2);
    v.extend_from_slice(l);
    v.extend_from_slice(r);
    rms_db(&v)
}

/// AutoLevel with the exact POXVoice params (flavor.rs:165 — identical
/// to AutoLevelNode::new()'s own constructor defaults, no override).
fn new_poxvoice_autolevel(sr: u32) -> AutoLevelNode {
    let mut n = AutoLevelNode::new(sr);
    n.set_params(-18.0, 500.0, 6.0, -6.0, 50.0);
    n
}

fn activation_fraction(dl: &[f32], dr: &[f32], sr: u32) -> f64 {
    let chunk_len = ((sr as f64) * ACT_CHUNK_MS / 1000.0).round().max(1.0) as usize;
    let n_chunks = dl.len() / chunk_len;
    if n_chunks == 0 {
        return f64::NAN;
    }
    let mut active = 0usize;
    for c in 0..n_chunks {
        let s = c * chunk_len;
        let e = s + chunk_len;
        let mut peak = 0.0_f32;
        for i in s..e {
            peak = peak.max(dl[i].abs()).max(dr[i].abs());
        }
        let dbfs = if peak > 0.0 { 20.0 * (peak as f64).log10() } else { f64::NEG_INFINITY };
        if dbfs > ACT_DBFS_THRESHOLD {
            active += 1;
        }
    }
    active as f64 / n_chunks as f64
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    assert!(!paths.is_empty(), "usage: autolevel_chain <file1> [file2] ...");

    let thresholds: [f32; 4] = [-30.0, -24.0, -18.0, -12.0];

    println!("=== ΜΕΤΡΗΣΗ Α — AutoLevel μόνο του, RMS εξόδου ===");
    println!("{:<45}  {:>12}  {:>12}", "αρχείο", "in_rms_db", "out_rms_db");
    let mut out_rms_values: Vec<(String, f64)> = Vec::new();
    for path in &paths {
        let (l0, r0, sr) = decode_stereo_native(path);
        let in_rms = combined_rms_db(&l0, &r0);

        let mut l = l0.clone();
        let mut r = r0.clone();
        let mut al = new_poxvoice_autolevel(sr);
        al.process_stereo(&mut l, &mut r);
        let out_rms = combined_rms_db(&l, &r);

        println!("{:<45}  {:>12.4}  {:>12.4}", path, in_rms, out_rms);
        out_rms_values.push((path.clone(), out_rms));
    }
    let spread = {
        let vals: Vec<f64> = out_rms_values.iter().map(|(_, v)| *v).collect();
        vals.iter().cloned().fold(f64::MIN, f64::max) - vals.iter().cloned().fold(f64::MAX, f64::min)
    };
    println!("spread (max-min) out_rms_db = {spread:.4} dB");
    println!();

    println!("=== ΜΕΤΡΗΣΗ Β — AutoLevel→DeEsser (σειρά αλυσίδας), % ενεργοποίησης ===");
    for path in &paths {
        println!("──────────────────────────────────────────────────────");
        println!("input: {path}");
        let (l0, r0, sr) = decode_stereo_native(path);

        // Στάδιο 1: AutoLevel (POXVoice params) — ΙΔΙΟ intermediate
        // buffer ξαναχρησιμοποιείται για όλα τα thresholds του DeEsser,
        // αφού το AutoLevel δεν εξαρτάται από το threshold.
        let mut leveled_l = l0.clone();
        let mut leveled_r = r0.clone();
        let mut al = new_poxvoice_autolevel(sr);
        al.process_stereo(&mut leveled_l, &mut leveled_r);
        let leveled_rms = combined_rms_db(&leveled_l, &leveled_r);
        println!("μετά το AutoLevel: rms_db = {leveled_rms:.4}");
        println!();

        println!("{:>8}  {:>14}  {:>10}", "thr(dB)", "null_rms_db", "activation");
        for &thr in &thresholds {
            let mut l = leveled_l.clone();
            let mut r = leveled_r.clone();
            let mut de = DeEsserNode::new(sr);
            de.set_parameter("threshold_db", thr);
            de.set_parameter("frequency_hz", 6000.0);
            de.set_parameter("ratio", 4.0);
            de.process_stereo(&mut l, &mut r);

            let mut diff_combined = Vec::with_capacity(l.len() * 2);
            let mut dl = Vec::with_capacity(l.len());
            let mut dr = Vec::with_capacity(r.len());
            for i in 0..l.len() {
                let vl = leveled_l[i] - l[i];
                let vr = leveled_r[i] - r[i];
                diff_combined.push(vl);
                diff_combined.push(vr);
                dl.push(vl);
                dr.push(vr);
            }
            let null_rms_db = rms_db(&diff_combined);
            let activation_frac = activation_fraction(&dl, &dr, sr);

            println!(
                "{:>8.1}  {:>14.4}  {:>9.2}%",
                thr, null_rms_db, activation_frac * 100.0
            );
        }
        println!();
    }
}
