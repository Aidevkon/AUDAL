//! MEASURE 2026-09-16: τα πιο επίπεδα κομμάτια του FMA allowlist v2.
//! Read-only, ΜΗΔΕΝ αλλαγή στην παραγωγή, ΜΗΔΕΝ νέο υλικό (μόνο τα
//! 437 tracks του allowlist).
//!
//! ΓΙΑΤΙ: η επιπεδότητα (F-115) δεν ξέρει από φωνή — ξέρει «πόσο
//! θορυβώδες». Δουλεύει στα δοκίμια επειδή τα δύο δικά μας bed είναι
//! τονικά. Το ACX δέχεται SFX ρητά (βροχή, πλήθος, άνεμος) — εκεί η
//! επιπεδότητα θα ήταν υψηλή χωρίς καμία φωνή. Ψάχνουμε με το ΟΡΓΑΝΟ
//! (μετρημένη επιπεδότητα), όχι με το δηλωμένο genre (ετικέτα, όχι
//! τεκμήριο — βλ. bed1 παρακάτω).
//!
//! ΤΙ ΜΕΤΡΑΤΑΙ: για κάθε track του allowlist, το ΙΔΙΟ όργανο/παράθυρο
//! με F-115/flatness_relative.rs/flatness_asymmetric_battery.rs —
//! per-window flatness = διάμεσος spectral_flatness() (μαύρο κουτί,
//! sp314-dsp/src/analysis/spectral.rs) σε 10 υπο-τμήματα 0.5s, ανά
//! παράθυρο 5s/hop 1s σε ΟΛΟ το track (fma_small preview clips, ~30s
//! μέσος όρος — 3.64h/437 tracks — άρα ΔΕΝ χρειάστηκε το 60s-απόσπασμα
//! fallback, ολόκληρο κάθε clip σαρώνεται). Το «track flatness» =
//! διάμεσος όλων των per-window τιμών του track.
//!
//! Αναγνώστης: m0d::dsp::input_lufs::pass0_decode_to_dump — η ΠΡΑΓΜΑΤΙΚΗ
//! production decode συνάρτηση (P0-a), ΙΔΙΑ με real_files_battery.rs/
//! flatness_relative.rs για τα εννιά βιβλία — όχι αναπαραγωγή decode.
//!
//! ⚠ ΔΙΟΡΘΩΣΗ ΠΡΟΗΓΟΥΜΕΝΗΣ ΚΑΤΑΓΡΑΦΗΣ (μετρημένο εδώ, ΟΧΙ διορθωμένο
//! στην πηγή — read-only task): fixture-battery-20260916.txt:52-53
//! έγραψε bed1 (fma_small/007/007481.mp3) ως genre «Noise» με τίτλο
//! σχετικό με percussion. Το ζωντανό allowlist JSON (track_id 7481,
//! license_raw, διαβασμένο αυτούσιο εδώ) δίνει genre «Hip-Hop», τίτλος
//! «Seeing You Out» (BrokeMC) — ΟΧΙ «Noise», ΟΧΙ percussion. Η
//! ασυμφωνία επιβεβαιώνει το ίδιο το μάθημα του task ένα επίπεδο πιο
//! κάτω: ούτε καν η ΠΑΛΙΑ ετικέτα ήταν σωστά καταγεγραμμένη.
//!
//! ΧΡΗΣΗ: cargo run --release --bin flatness_allowlist_survey

use serde::Deserialize;
use sp314_dsp::analysis::spectral::spectral_flatness;

const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const SUBCHUNK_SECS: f32 = 0.5;
const DUMP_FRAME_BYTES: usize = 8;
const NARRATION_FLOOR: f32 = 0.0888; // σημερινό ελάχιστο σκέτης αφήγησης (task premise)

const BED1_TRACK_ID: &str = "7481";
const BED2_TRACK_ID: &str = "114411";

#[derive(Deserialize)]
struct AllowlistEntry {
    track_id: String,
    path: String,
    license_raw: String,
}

fn genre_of(license_raw: &str) -> String {
    license_raw
        .split('|')
        .nth(5)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn title_of(license_raw: &str) -> String {
    license_raw.split('|').next().map(|s| s.trim().to_string()).unwrap_or_else(|| "?".to_string())
}

fn read_dump_stereo(path: &str) -> (Vec<f32>, Vec<f32>) {
    let buf = std::fs::read(path).unwrap_or_else(|e| panic!("read dump {path}: {e}"));
    let full_frames = buf.len() / DUMP_FRAME_BYTES;
    let mut left = Vec::with_capacity(full_frames);
    let mut right = Vec::with_capacity(full_frames);
    for i in 0..full_frames {
        let base = i * DUMP_FRAME_BYTES;
        left.push(f32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]));
        right.push(f32::from_le_bytes([buf[base + 4], buf[base + 5], buf[base + 6], buf[base + 7]]));
    }
    (left, right)
}

fn median(xs: &mut [f32]) -> f32 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = xs.len();
    if n == 0 {
        return f32::NAN;
    }
    if n % 2 == 0 { (xs[n / 2 - 1] + xs[n / 2]) / 2.0 } else { xs[n / 2] }
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return f32::NAN;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f32).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// ΙΔΙΟΣ ορισμός με F-115/flatness_relative.rs: διάμεσος spectral_flatness()
/// σε 10 υπο-τμήματα 0.5s.
fn window_flatness(mono_slice: &[f32], sample_rate: u32) -> f32 {
    let sub_samples = (SUBCHUNK_SECS * sample_rate as f32) as usize;
    let mut vals = Vec::new();
    let mut pos = 0;
    while pos + sub_samples <= mono_slice.len() {
        vals.push(spectral_flatness(&mono_slice[pos..pos + sub_samples]));
        pos += sub_samples;
    }
    median(&mut vals)
}

struct TrackResult {
    track_id: String,
    path: String,
    genre: String,
    title: String,
    flatness: f32,
    n_windows: usize,
}

fn measure_track(entry: &AllowlistEntry, idx: usize, total: usize) -> Option<TrackResult> {
    let dump = format!("/tmp/flatness_allowlist_{}.raw", entry.track_id);
    if let Err(e) = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(&entry.path), &dump) {
        eprintln!("[{}/{}] ΠΑΡΑΛΕΙΠΕΤΑΙ {} ({}): decode failed: {e}", idx + 1, total, entry.track_id, entry.path);
        return None;
    }
    let (left, right) = read_dump_stereo(&dump);
    let _ = std::fs::remove_file(&dump);
    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

    let mut flats = Vec::new();
    let mut start = 0usize;
    while start + win_samples <= mono.len() {
        flats.push(window_flatness(&mono[start..start + win_samples], sample_rate));
        start += hop_samples;
    }

    if flats.is_empty() {
        eprintln!("[{}/{}] ΠΑΡΑΛΕΙΠΕΤΑΙ {} ({}): < 5s, μηδέν πλήρη παράθυρα", idx + 1, total, entry.track_id, entry.path);
        return None;
    }

    let track_flatness = median(&mut flats.clone());
    if (idx + 1) % 50 == 0 {
        eprintln!("[{}/{}] ...", idx + 1, total);
    }
    Some(TrackResult {
        track_id: entry.track_id.clone(),
        path: entry.path.clone(),
        genre: genre_of(&entry.license_raw),
        title: title_of(&entry.license_raw),
        flatness: track_flatness,
        n_windows: flats.len(),
    })
}

fn main() {
    let allowlist_path = "/home/aidevcon/Downloads/DATASET/fma/fma_small_cc_allowlist_v2.json";
    let raw = std::fs::read_to_string(allowlist_path).unwrap_or_else(|e| panic!("read {allowlist_path}: {e}"));
    let entries: Vec<AllowlistEntry> = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse json: {e}"));
    let total = entries.len();
    println!("allowlist: {allowlist_path}  entries={total}");
    println!("παράθυρο={WINDOW_SECS}s hop={HOP_SECS}s υπο-τμήμα={SUBCHUNK_SECS}s  narration_floor={NARRATION_FLOOR}\n");

    let mut results: Vec<TrackResult> = Vec::with_capacity(total);
    for (i, entry) in entries.iter().enumerate() {
        if let Some(r) = measure_track(entry, i, total) {
            results.push(r);
        }
    }

    let skipped = total - results.len();
    println!("\nμετρημένα={}/{total}  παραλείφθηκαν={skipped}\n", results.len());

    let mut values: Vec<f32> = results.iter().map(|r| r.flatness).collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = median(&mut values.clone());
    let p10 = percentile(&values, 10.0);
    let p90 = percentile(&values, 90.0);
    let max = *values.last().unwrap();
    let min = *values.first().unwrap();

    println!("=== ΚΑΤΑΝΟΜΗ (n={}) ===", values.len());
    println!("  ελάχιστο={:.4}  p10={:.4}  διάμεσος={:.4}  p90={:.4}  μέγιστο={:.4}", min, p10, med, p90, max);

    let above = results.iter().filter(|r| r.flatness > NARRATION_FLOOR).count();
    println!("\n=== ΤΟ ΚΡΙΣΙΜΟ ===");
    println!("  κομμάτια με διάμεσο > {:.4} (ελάχιστο σκέτης αφήγησης σήμερα): {} / {}", NARRATION_FLOOR, above, results.len());

    let mut sorted_desc: Vec<&TrackResult> = results.iter().collect();
    sorted_desc.sort_by(|a, b| b.flatness.partial_cmp(&a.flatness).unwrap());

    println!("\n=== ΤΑ ΔΕΚΑ ΠΙΟ ΕΠΙΠΕΔΑ ===");
    for (rank, r) in sorted_desc.iter().take(10).enumerate() {
        println!(
            "  {:>2}. flat={:.4}  n_win={:<3}  track_id={:<8}  genre={:<16}  {}",
            rank + 1, r.flatness, r.n_windows, r.track_id, r.genre, r.title
        );
    }

    println!("\n=== ΤΑ ΔΥΟ ΔΙΚΑ ΜΑΣ bed, ΓΙΑ ΘΕΣΗ ===");
    for (rank, r) in sorted_desc.iter().enumerate() {
        if r.track_id == BED1_TRACK_ID || r.track_id == BED2_TRACK_ID {
            println!(
                "  θέση {:>3}/{}  flat={:.4}  n_win={:<3}  track_id={:<8}  genre={:<16}  {}",
                rank + 1, sorted_desc.len(), r.flatness, r.n_windows, r.track_id, r.genre, r.title
            );
        }
    }
}
