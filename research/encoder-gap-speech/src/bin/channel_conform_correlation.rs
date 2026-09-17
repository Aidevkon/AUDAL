//! RECON 2026-09-18: το channel conform — correlation και Δ(RMS) του
//! mono downmix, σε υλικό με χαμηλή συσχέτιση καναλιών (FMA allowlist,
//! ΟΧΙ αφήγηση — δηλωμένο όριο). Read-only, ΜΗΔΕΝ αλλαγή στην
//! παραγωγή, ΜΗΔΕΝ νέο υλικό εκτός allowlist.
//!
//! ΓΙΑΤΙ FMA: τα εννιά βιβλία αφήγησης δεν έχουν το δύσκολο άκρο (μόνο
//! ένα πραγματικά στέρεο, correlation 0.997). Το FMA allowlist (437
//! κομμάτια μουσικής) έχει το εύρος συσχέτισης που λείπει.
//!
//! ΧΡΗΣΗ: cargo run --release --bin channel_conform_correlation
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use sp314_dsp::analysis::stereo::stereo_correlation;
use serde::Deserialize;

const SR: u32 = 48_000;
const DUMP_FRAME_BYTES: usize = 8;

#[derive(Deserialize)]
struct AllowlistEntry {
    track_id: String,
    path: String,
}

fn read_dump_stereo(path: &str) -> (Vec<f32>, Vec<f32>) {
    let buf = std::fs::read(path).unwrap_or_else(|e| panic!("read dump {path}: {e}"));
    let n = buf.len() / DUMP_FRAME_BYTES;
    let mut l = Vec::with_capacity(n);
    let mut r = Vec::with_capacity(n);
    for i in 0..n {
        let base = i * DUMP_FRAME_BYTES;
        l.push(f32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]));
        r.push(f32::from_le_bytes([buf[base + 4], buf[base + 5], buf[base + 6], buf[base + 7]]));
    }
    (l, r)
}

fn stereo_pooled_rms_db(l: &[f32], r: &[f32]) -> f32 {
    let mut a = AcxCheckAnalyzer::new(SR);
    a.feed_chunk(l);
    a.feed_chunk(r);
    a.finish().rms_db
}

fn mono_downmix_rms_db(l: &[f32], r: &[f32]) -> f32 {
    let mono: Vec<f32> = l.iter().zip(r.iter()).map(|(&a, &b)| (a + b) * 0.5).collect();
    let mut a = AcxCheckAnalyzer::new(SR);
    a.feed_chunk(&mono);
    a.finish().rms_db
}

fn main() {
    let allowlist_path = "/home/aidevcon/Downloads/DATASET/fma/fma_small_cc_allowlist_v2.json";
    let raw = std::fs::read_to_string(allowlist_path).unwrap_or_else(|e| panic!("read {allowlist_path}: {e}"));
    let entries: Vec<AllowlistEntry> = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {allowlist_path}: {e}"));
    let total = entries.len();
    println!("allowlist: {allowlist_path}  entries={total}\n");

    let mut correlations: Vec<f32> = Vec::with_capacity(total);
    let mut low_corr_rows: Vec<(String, f32, f32, f32, f32)> = Vec::new(); // track_id, corr, stereo_rms, mono_rms, diff
    let mut skipped = 0usize;

    for (idx, entry) in entries.iter().enumerate() {
        let dump = format!("/tmp/chconform_{}.raw", entry.track_id);
        if let Err(e) = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(&entry.path), &dump) {
            eprintln!("[{}/{}] ΠΑΡΑΛΕΙΠΕΤΑΙ {} ({}): decode failed: {e}", idx + 1, total, entry.track_id, entry.path);
            skipped += 1;
            continue;
        }
        let (l, r) = read_dump_stereo(&dump);
        let _ = std::fs::remove_file(&dump);
        if l.len() < SR as usize {
            skipped += 1;
            continue;
        }

        let corr = stereo_correlation(&l, &r);
        correlations.push(corr);

        if corr < 0.5 && low_corr_rows.len() < 10 {
            let stereo_rms = stereo_pooled_rms_db(&l, &r);
            let mono_rms = mono_downmix_rms_db(&l, &r);
            low_corr_rows.push((entry.track_id.clone(), corr, stereo_rms, mono_rms, mono_rms - stereo_rms));
        }

        if (idx + 1) % 50 == 0 {
            eprintln!("[{}/{}] ...", idx + 1, total);
        }
    }

    println!("μετρήθηκαν={} παραλείφθηκαν={}\n", correlations.len(), skipped);

    // Κατανομή συσχέτισης — δεκάδες bins [-1,1].
    let mut sorted = correlations.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("=== ΚΑΤΑΝΟΜΗ ΣΥΣΧΕΤΙΣΗΣ, {} ΚΟΜΜΑΤΙΑ ===", sorted.len());
    let bins = [(-1.0, -0.5), (-0.5, 0.0), (0.0, 0.5), (0.5, 0.8), (0.8, 0.95), (0.95, 0.999), (0.999, 1.0001)];
    for (lo, hi) in bins {
        let n = sorted.iter().filter(|&&c| c >= lo && c < hi).count();
        let pct = 100.0 * n as f32 / sorted.len().max(1) as f32;
        println!("  [{lo:>7.3}, {hi:<7.3}) : {n:>4}  ({pct:>5.1}%)");
    }
    let median = if sorted.is_empty() { f32::NAN } else { sorted[sorted.len() / 2] };
    let p10 = if sorted.is_empty() { f32::NAN } else { sorted[sorted.len() / 10] };
    println!("  median={median:.4}  p10={p10:.4}  min={:.4}  max={:.4}", sorted.first().copied().unwrap_or(f32::NAN), sorted.last().copied().unwrap_or(f32::NAN));
    let n_below_05 = sorted.iter().filter(|&&c| c < 0.5).count();
    println!("  correlation < 0.5: {n_below_05}/{} ({:.1}%)\n", sorted.len(), 100.0 * n_below_05 as f32 / sorted.len().max(1) as f32);

    println!("=== ΜΕΧΡΙ ΔΕΚΑ ΚΟΜΜΑΤΙΑ ΜΕ correlation < 0.5 — Δ(RMS) mono downmix ===\n");
    if low_corr_rows.is_empty() {
        println!("ΚΑΝΕΝΑ κομμάτι με correlation < 0.5 βρέθηκε στο allowlist.");
    } else {
        for (track_id, corr, stereo_rms, mono_rms, diff) in &low_corr_rows {
            println!(
                "track {track_id:<10} correlation={corr:>7.4}  στέρεο(pooled)={stereo_rms:>7.3}dB  mono(downmix)={mono_rms:>7.3}dB  Δ={diff:>+6.3}dB"
            );
        }
        let max_abs_diff = low_corr_rows.iter().map(|r| r.4.abs()).fold(0.0_f32, f32::max);
        println!("\nΜέγιστο |Δ| στα χαμηλής-συσχέτισης: {max_abs_diff:.3}dB");
    }
}
