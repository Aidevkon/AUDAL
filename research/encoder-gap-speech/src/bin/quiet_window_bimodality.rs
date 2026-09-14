//! ΜΕΤΡΗΣΗ: είναι η κατανομή των ήσυχων παραθύρων διμερής;
//!
//! ΓΙΑΤΙ: τα δύο υποψήφια φράγματα (interior · quietest_active) μετρήθηκαν
//! 14/09 να απέχουν 0.14–3.94 dB σε 8/9 αρχεία και να αντιστρέφονται στο
//! ένατο. Δεν υπάρχει «ανάμεσα». Το ερώτημα μετατοπίζεται: υπάρχει κοιλάδα
//! στην ΙΔΙΑ την κατανομή;
//!
//! ΤΟ ΣΗΜΑ: το DUMP της αλυσίδας (48k stereo f32 LE, mono downmix
//! `(l+r)*0.5` — ΙΔΙΟΣ τύπος με trunk_pass.rs:509). Ό,τι βλέπει ο expander,
//! όχι το mp3.
//!
//! ΜΗΔΕΝ ΕΞΟΜΑΛΥΝΣΗ. Οι κορυφές μετριούνται στο ωμό ιστόγραμμα.
//!
//! ΧΡΗΣΗ: cargo run --release --bin quiet_window_bimodality -- <audio>...

const LO_DB: i32 = -100;
const HI_DB: i32 = 0;
const NBINS: usize = (HI_DB - LO_DB) as usize; // 100 κάδοι του 1 dB

fn dump_mono(path: &str, tag: &str) -> (Vec<f32>, Option<f32>, Option<f32>) {
    let dump = format!("/tmp/bimodal_{tag}.raw");
    let _ = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .expect("pass0");
    let edge = lineos_types::presets::ACX.room_tone_max_s.unwrap();
    let r = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
        std::path::Path::new(&dump), false, edge).expect("trunk");
    let interior = r.acx_interior_noise_floor.map(|(db, _)| db);
    let quietest = r.quietest_active_window_dbfs;

    let bytes = std::fs::read(&dump).expect("read dump");
    let n = bytes.len() / 4;
    let mut inter = Vec::with_capacity(n);
    for i in 0..n {
        inter.push(f32::from_le_bytes([
            bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3]]));
    }
    let mono: Vec<f32> = inter.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect();
    let _ = std::fs::remove_file(&dump);
    (mono, interior, quietest)
}

/// RMS ανά 100 ms σε dBFS.
fn rms_100ms(x: &[f32]) -> Vec<f32> {
    let w = 4_800usize; // 100 ms @ 48k
    x.chunks(w)
        .filter(|c| c.len() == w)
        .map(|c| {
            let e = c.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / w as f64;
            if e < 1e-20 { -200.0 } else { (10.0 * e.log10()) as f32 }
        })
        .collect()
}

fn histogram(v: &[f32]) -> [u64; NBINS] {
    let mut h = [0u64; NBINS];
    for &x in v {
        let b = (x - LO_DB as f32).floor() as i32;
        if b >= 0 && (b as usize) < NBINS {
            h[b as usize] += 1;
        }
    }
    h
}

/// Otsu σε 1-D ιστόγραμμα. Μηδέν παράμετρος: μεγιστοποιεί τη διακύμανση
/// ΑΝΑΜΕΣΑ στις δύο κλάσεις. Επιστρέφει το dBFS της τομής.
fn otsu(h: &[u64; NBINS]) -> f32 {
    let total: f64 = h.iter().map(|&c| c as f64).sum();
    if total == 0.0 { return f32::NAN; }
    let sum_all: f64 = h.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut w0, mut sum0) = (0.0_f64, 0.0_f64);
    let (mut best_var, mut best_t) = (-1.0_f64, 0usize);
    for t in 0..NBINS {
        w0 += h[t] as f64;
        if w0 == 0.0 { continue; }
        let w1 = total - w0;
        if w1 == 0.0 { break; }
        sum0 += t as f64 * h[t] as f64;
        let m0 = sum0 / w0;
        let m1 = (sum_all - sum0) / w1;
        let var = w0 * w1 * (m0 - m1) * (m0 - m1);
        if var > best_var { best_var = var; best_t = t; }
    }
    LO_DB as f32 + best_t as f32 + 0.5
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    assert!(!files.is_empty(), "usage: quiet_window_bimodality <audio>...");
    let limit = lineos_types::presets::ACX.max_noise_floor_db.unwrap();

    println!("κάδοι 1 dB, {LO_DB}..{HI_DB} · παράθυρα 100 ms · σήμα: DUMP της αλυσίδας");
    println!("ΜΗΔΕΝ εξομάλυνση\n");

    let mut rows = Vec::new();
    for path in &files {
        let stem = std::path::Path::new(path).file_stem().unwrap()
            .to_string_lossy().into_owned();
        let (mono, interior, quietest) = dump_mono(path, &stem);
        let r = rms_100ms(&mono);
        let h = histogram(&r);
        let total: u64 = h.iter().sum();
        let hmax = *h.iter().max().unwrap_or(&1);

        println!("═══════════════════════════════════════════════════════════");
        println!("{stem}   παράθυρα {total}");

        // ── ΑΣCII ιστόγραμμα, μόνο μη-κενοί κάδοι ──
        for (i, &c) in h.iter().enumerate() {
            if c == 0 { continue; }
            let db = LO_DB + i as i32;
            let w = (c as f64 / hmax as f64 * 56.0).round() as usize;
            println!("  {db:>4} {:>6} {}", c, "#".repeat(w.max(1)));
        }

        // ── ΩΜΕΣ τοπικές κορυφές ──
        let mut peaks = Vec::new();
        for i in 0..NBINS {
            let l = if i == 0 { 0 } else { h[i - 1] };
            let rr = if i + 1 == NBINS { 0 } else { h[i + 1] };
            if h[i] > 0 && h[i] > l && h[i] >= rr { peaks.push(i); }
        }

        // ── Otsu, και οι δύο κορυφές εκατέρωθέν του ──
        let t_db = otsu(&h);
        let t_bin = (t_db - LO_DB as f32).floor() as usize;
        let lo_peak = (0..t_bin.min(NBINS)).max_by_key(|&i| h[i]);
        let hi_peak = (t_bin.min(NBINS)..NBINS).max_by_key(|&i| h[i]);
        let valley = match (lo_peak, hi_peak) {
            (Some(a), Some(b)) if a < b => (a..=b).map(|i| h[i]).min().unwrap_or(0),
            _ => 0,
        };
        let smaller_peak = match (lo_peak, hi_peak) {
            (Some(a), Some(b)) => h[a].min(h[b]),
            _ => 0,
        };
        let ratio = if smaller_peak > 0 { valley as f64 / smaller_peak as f64 } else { f64::NAN };

        let below = r.iter().filter(|&&v| v < t_db).count();
        let pct = below as f64 / r.len() as f64 * 100.0;

        println!("  ΩΜΕΣ τοπικές κορυφές: {}", peaks.len());
        println!("  Otsu: {t_db:.1} dBFS");
        if let (Some(a), Some(b)) = (lo_peak, hi_peak) {
            println!("    κορυφή κάτω {:>4} dB ({}) · κορυφή πάνω {:>4} dB ({})",
                LO_DB + a as i32, h[a], LO_DB + b as i32, h[b]);
            println!("    κοιλάδα {valley} · μικρότερη κορυφή {smaller_peak} · λόγος {ratio:.4}");
        }
        println!("  interior {:?} · quietest_active {:?} · όριο {limit}", interior, quietest);
        if let Some(f) = interior {
            println!("  otsu ΠΑΝΩ από interior: {:.2} dB", t_db - f);
        }
        println!("  κάτω από otsu: {below}/{} = {pct:.1}%", r.len());
        println!();

        rows.push((stem, t_db, interior, quietest, peaks.len(), ratio, pct));
    }

    println!("═══ ΣΥΝΟΨΗ ═══");
    println!("{:<36}{:>8}{:>10}{:>11}{:>7}{:>8}{:>8}",
        "αρχείο", "otsu", "interior", "quietest", "κορυφ", "λόγος", "%κάτω");
    for (s, t, i, q, p, r, pc) in &rows {
        println!("{:<36}{:>8.1}{:>10.2}{:>11.2}{:>7}{:>8.3}{:>7.1}%",
            s, t, i.unwrap_or(f32::NAN), q.unwrap_or(f32::NAN), p, r, pc);
    }
    println!("\nΔιμερή κατά Otsu-split (λόγος κοιλάδας < 0.5): {}/{}",
        rows.iter().filter(|r| r.5 < 0.5).count(), rows.len());
}
