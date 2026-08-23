//! SPACING ΒΗΜΑ Γ — μέτρηση κατανομής, ΟΧΙ gate, ΟΧΙ implement.
//!
//! Υλικό: "24 - The Wise In The Desert.flac" (το ίδιο qualifying candidate
//! από .reports/2026-08-23-noise-floor-gap.md — 44.1kHz stereo, 154.88s,
//! 35 πραγματικές παύσεις με room tone, RMS 0.000056, όχι ψηφιακή σιωπή).
//!
//! ΕΡΩΤΗΜΑ: υπάρχει ΚΑΘΑΡΟ κενό στην κατανομή του "dB πάνω από το floor"
//! ανάμεσα σε (room tone) / (ανάσα) / (ομιλία); Αν ΝΑΙ και τα δύο κενά,
//! το X (ανάσα->ομιλία) βγαίνει από το ΔΕΥΤΕΡΟ. Αν ΟΧΙ: STOP, ανάφερε.
//!
//! ΟΡΓΑΝΟ: ίδιο HP8 @10Hz cascade + 100ms sub-block + 500ms/5-subblock
//! ελάχιστο παράθυρο με τον AcxCheckAnalyzer (acx_check.rs) — αναπαραγωγή
//! με τα ΔΗΜΟΣΙΑ primitives (butter_hp2_q, Biquad), ΙΔΙΟ μοτίβο με
//! noise_floor_case.rs's "ΘΕΣΗ ΤΟΥ ΗΣΥΧΟΤΕΡΟΥ ΠΑΡΑΘΥΡΟΥ" — όχι νέος
//! αλγόριθμος. Νative sample rate (44.1kHz), mono downmix, ΧΩΡΙΣ encode/
//! decode: το spacing margin είναι 0.0 (καμία encoder-χρονική μετατόπιση
//! να αντισταθμιστεί), άρα η καθαρή πηγή αρκεί και είναι το πιο άμεσο
//! μέτρημα.
//!
//! ΔΕΝ αγγίζει product code. Standalone, atomic single-script.

use sp314_dsp::analysis::pre_analysis::{butter_hp2_q, Biquad};
use std::path::PathBuf;

const HP_CUTOFF_HZ: f32 = 10.0;
const SUB_BLOCK_MS: f32 = 100.0;
const WINDOW_SUB_BLOCKS: usize = 5; // 500ms, ΙΔΙΟ με acx_check.rs

fn decode_to_mono_f32(path: &str) -> (Vec<f32>, u32) {
    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v", "error", "-show_entries", "stream=sample_rate",
            "-of", "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .expect("ffprobe spawn");
    let sr: u32 = String::from_utf8_lossy(&probe.stdout)
        .lines()
        .next()
        .expect("no sr")
        .trim()
        .parse()
        .expect("bad sr");

    let tmp = PathBuf::from(format!("/tmp/spacing_distribution_decode_{}.pcm", std::process::id()));
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(path)
        .args(["-ac", "1", "-f", "f32le"])
        .arg(&tmp)
        .status()
        .expect("ffmpeg spawn");
    assert!(status.success(), "ffmpeg decode failed");
    let bytes = std::fs::read(&tmp).expect("read decoded pcm");
    let _ = std::fs::remove_file(&tmp);
    let samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    (samples, sr)
}

fn main() {
    let default_input = "/home/aidevcon/Downloads/DATASET/24 - The Wise In The Desert.flac".to_string();
    let input = std::env::args().nth(1).unwrap_or(default_input);
    let input = input.as_str();
    let (mono, sr) = decode_to_mono_f32(input);
    let dur = mono.len() as f64 / sr as f64;
    println!("input: {input}");
    println!("sr={sr} samples={} dur={dur:.2}s", mono.len());

    // ΙΔΙΟ HP8 cascade με acx_check.rs::AcxCheckAnalyzer::new (Qs από τις
    // ίδιες pole-angle θ_k = (2k+1)*π/16, ίδιο τύπωμα).
    let mut hp: [Biquad; 4] = core::array::from_fn(|k| {
        let theta = (2 * k + 1) as f32 * core::f32::consts::PI / 16.0;
        let q = 1.0 / (2.0 * theta.cos());
        butter_hp2_q(HP_CUTOFF_HZ, sr as f32, q)
    });
    let mut filtered = Vec::with_capacity(mono.len());
    for &s in &mono {
        let mut y = s;
        for section in hp.iter_mut() {
            y = section.process(y);
        }
        filtered.push(y);
    }

    // Per-100ms sub-block mean square -> dB, χρονικά διατεταγμένο —
    // ΑΚΡΙΒΩΣ το sub_mean_sqs που ο AcxCheckAnalyzer κρατά εσωτερικά.
    let sub_block_size = (sr as f32 * SUB_BLOCK_MS / 1000.0) as usize;
    let num_sub_blocks = filtered.len() / sub_block_size;
    let mut sub_db: Vec<f32> = Vec::with_capacity(num_sub_blocks);
    for i in 0..num_sub_blocks {
        let block = &filtered[i * sub_block_size..(i + 1) * sub_block_size];
        let mean_sq: f64 = block.iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>()
            / block.len() as f64;
        let rms = mean_sq.sqrt();
        let db = if rms < 1e-10 { -144.0 } else { 20.0 * rms.log10() as f32 };
        sub_db.push(db);
    }
    println!("sub-blocks (100ms): {num_sub_blocks}");

    // Global noise_floor_db — ΙΔΙΟΣ ορισμός: ελάχιστο 500ms (5 διαδοχικά
    // sub-blocks) mean-square παράθυρο, sqrt -> dB. Reference point για
    // "dB πάνω από το floor".
    let mut min_window_ms: f64 = f64::MAX;
    for i in 0..=(num_sub_blocks.saturating_sub(WINDOW_SUB_BLOCKS)) {
        let window = &sub_db[i..i + WINDOW_SUB_BLOCKS];
        // ξαναφτιάχνουμε mean-square από τα dB (πάμε πίσω σε lin, μέσος,
        // όχι μέσος όρος των dB — ΙΔΙΟ με το πραγματικό όργανο).
        let mean_sq: f64 = window
            .iter()
            .map(|&db| {
                let amp = 10f64.powf(db as f64 / 20.0);
                amp * amp
            })
            .sum::<f64>()
            / window.len() as f64;
        if mean_sq < min_window_ms {
            min_window_ms = mean_sq;
        }
    }
    let noise_floor_db = 20.0 * min_window_ms.sqrt().log10();
    println!("noise_floor_db (500ms min window, ίδιος ορισμός με AcxCheckAnalyzer): {noise_floor_db:.4}");

    // dB πάνω από το floor, ανά sub-block.
    let above_floor: Vec<f32> = sub_db.iter().map(|&db| db - noise_floor_db as f32).collect();

    // Ιστόγραμμα σε bins του 1dB, από 0 μέχρι το observed max.
    let max_above = above_floor.iter().cloned().fold(0.0f32, f32::max);
    let num_bins = (max_above.ceil() as usize) + 1;
    let mut hist = vec![0usize; num_bins + 1];
    for &v in &above_floor {
        let bin = (v.max(0.0).floor() as usize).min(num_bins);
        hist[bin] += 1;
    }

    println!("\n=== ΙΣΤΟΓΡΑΜΜΑ: sub-blocks ανά bin (dB πάνω από floor, 1dB bins) ===");
    for (bin, &count) in hist.iter().enumerate() {
        let bar = "#".repeat((count as f64).sqrt().round() as usize);
        println!("  [{bin:>3}, {:>3}) dB: {count:>5}  {bar}", bin + 1);
    }

    // Εντοπισμός ΚΕΝΩΝ: διαδοχικά bins με μηδέν sub-blocks, μετά την
    // πρώτη μη-μηδενική τιμή (ξεκινάμε πάντα από bin 0, το floor).
    println!("\n=== ΚΕΝΑ (διαδοχικά άδεια bins μετά το πρώτο γεμάτο) ===");
    let mut gaps: Vec<(usize, usize)> = Vec::new();
    let mut in_gap = false;
    let mut gap_start = 0usize;
    let mut seen_first_nonzero = false;
    for (bin, &count) in hist.iter().enumerate() {
        if count > 0 {
            seen_first_nonzero = true;
            if in_gap {
                gaps.push((gap_start, bin));
                in_gap = false;
            }
        } else if seen_first_nonzero && !in_gap {
            in_gap = true;
            gap_start = bin;
        }
    }
    if gaps.is_empty() {
        println!("  ΚΑΝΕΝΑ κενό βρέθηκε — η κατανομή είναι συνεχής, χωρίς φυσικό διαχωρισμό.");
    } else {
        for (start, end) in &gaps {
            println!("  κενό: [{start}, {end}) dB πάνω από floor ({} άδεια bins)", end - start);
        }
    }

    // Στατιστικά ζωνών γύρω από τα ΔΗΛΩΜΕΝΑ όρια του BreathCut
    // (−60..−30 dBFS ΑΠΟΛΥΤΟ) — ΜΟΝΟ ως σημείο αναφοράς για σύγκριση,
    // ΟΧΙ ως πηγή του X (η εντολή είναι ρητή: μη δανειστείς αυτά τα
    // νούμερα). Τυπώνουμε πόσα sub-blocks πέφτουν εντός αυτής της
    // απόλυτης ζώνης, για να δούμε αν συμπίπτει καθόλου με ό,τι κενό
    // βρέθηκε παραπάνω.
    let breath_zone_count = sub_db.iter().filter(|&&db| db >= -60.0 && db <= -30.0).count();
    println!(
        "\n(αναφορά μόνο) sub-blocks μέσα στη ΔΗΛΩΜΕΝΗ ζώνη BreathCut [-60,-30] dBFS ΑΠΟΛΥΤΟ: {breath_zone_count}/{num_sub_blocks}"
    );

    println!("\n=== ΣΥΝΟΨΗ (κενά) ===");
    println!("noise_floor_db = {noise_floor_db:.4}");
    println!("μέγιστο dB πάνω από floor παρατηρημένο = {max_above:.2}");
    println!("αριθμός κενών βρεθέντων = {}", gaps.len());
    if gaps.len() >= 2 {
        let (_, second_gap_end) = gaps[1];
        println!(
            "ΑΝ αυτά είναι τα δύο αναμενόμενα κενά (room->breath, breath->speech): \
             X = {} dB (κάτω άκρο του ΔΕΥΤΕΡΟΥ κενού + 1, δηλ. όπου ξαναρχίζει η πυκνότητα)",
            second_gap_end
        );
    } else if gaps.len() == 1 {
        println!("ΜΟΝΟ ΕΝΑ κενό βρέθηκε — δεν διακρίνεται ανάσα από ομιλία ξεχωριστά.");
    } else {
        println!("ΚΑΝΕΝΑ κενό — ο ενεργειακός ορισμός δεν διακρίνει καθόλου.");
    }

    // ─────────────────────────────────────────────────────────────
    // ΣΥΝΕΧΕΙΑ ΒΗΜΑΤΟΣ Γ — 1: Η ΚΟΙΛΑΔΑ ΩΣ ΠΥΚΝΟΤΗΤΑ
    // Όχι "άδειο bin" — density (sub-blocks/dB) ανά ζώνη, για να
    // φανεί αν υπάρχει πτώση κατά τάξη μεγέθους (κοιλάδα) και όχι
    // μόνο απόλυτα άδεια bins.
    // ─────────────────────────────────────────────────────────────
    println!("\n=== 1. ΠΥΚΝΟΤΗΤΑ ΑΝΑ ΖΩΝΗ (sub-blocks / dB-εύρος) ===");
    let zone_a: usize = hist.get(0..1).map(|s| s.iter().sum()).unwrap_or(0);
    let zone_b_bins = &hist[1.min(hist.len())..42.min(hist.len())];
    let zone_b: usize = zone_b_bins.iter().sum();
    let zone_b_width = zone_b_bins.len();
    let zone_c_bins = &hist[42.min(hist.len())..hist.len().min(76)];
    let zone_c: usize = zone_c_bins.iter().sum();
    let zone_c_width = zone_c_bins.len();
    println!(
        "  [0,1) dB   (ζώνη A, room tone):   {zone_a:>4} sub-blocks σε 1 dB   -> density {:.2}/dB",
        zone_a as f64 / 1.0
    );
    println!(
        "  [1,41] dB  (ζώνη B, κοιλάδα;):    {zone_b:>4} sub-blocks σε {zone_b_width} dB  -> density {:.2}/dB",
        zone_b as f64 / zone_b_width as f64
    );
    println!(
        "  [42,75] dB (ζώνη C, ομιλία):      {zone_c:>4} sub-blocks σε {zone_c_width} dB  -> density {:.2}/dB",
        zone_c as f64 / zone_c_width as f64
    );
    let density_a = zone_a as f64 / 1.0;
    let density_b = zone_b as f64 / zone_b_width as f64;
    let density_c = zone_c as f64 / zone_c_width as f64;
    println!(
        "  λόγος πυκνότητας A/B = {:.1}x, C/B = {:.1}x (τάξη μεγέθους αν >~10x)",
        density_a / density_b.max(1e-9),
        density_c / density_b.max(1e-9)
    );
    // Peak density MΕΣΑ στη ζώνη C (όχι μέσος όρος) — για σωστή σύγκριση
    // με το μέγιστο της ζώνης B, αφού η C ανεβοκατεβαίνει έντονα.
    let zone_b_peak = zone_b_bins.iter().cloned().max().unwrap_or(0);
    let zone_c_peak = zone_c_bins.iter().cloned().max().unwrap_or(0);
    println!(
        "  peak bin B = {zone_b_peak} sub-blocks· peak bin C = {zone_c_peak} sub-blocks -> λόγος {:.1}x",
        zone_c_peak as f64 / zone_b_peak.max(1) as f64
    );

    // ─────────────────────────────────────────────────────────────
    // ΣΥΝΕΧΕΙΑ ΒΗΜΑΤΟΣ Γ — 2: ΤΟ ΤΡΙΤΟ ΠΛΑΙΣΙΟ (median ± X)
    // ─────────────────────────────────────────────────────────────
    let mut speech_values: Vec<f32> = above_floor.iter().cloned().filter(|&v| v > 42.0).collect();
    speech_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_speech = if speech_values.is_empty() {
        f32::NAN
    } else if speech_values.len() % 2 == 1 {
        speech_values[speech_values.len() / 2]
    } else {
        let mid = speech_values.len() / 2;
        (speech_values[mid - 1] + speech_values[mid]) / 2.0
    };
    println!("\n=== 2. ΔΙΑΜΕΣΟΣ ΤΗΣ ΜΑΖΑΣ ΟΜΙΛΙΑΣ (>42 dB πάνω από floor) ===");
    println!(
        "  n={} sub-blocks, διάμεσος = {median_speech:.2} dB πάνω από floor",
        speech_values.len()
    );

    let block_dur_s = SUB_BLOCK_MS as f64 / 1000.0;
    println!("\n=== head_spacing / tail_spacing για X κάτω από τη διάμεσο ===");
    println!("  {:>4}  {:>14}  {:>10}  {:>12}  {:>12}", "X", "threshold(dB)", "head_idx", "head_s", "tail_s");
    let mut spacing_by_x: Vec<(i32, f64, f64)> = Vec::new();
    for &x in &[10.0f32, 15.0, 20.0, 25.0, 30.0] {
        let threshold = median_speech - x;
        let head_idx = above_floor.iter().position(|&v| v >= threshold);
        let tail_idx = above_floor.iter().rposition(|&v| v >= threshold);
        let head_s = head_idx.map(|i| i as f64 * block_dur_s).unwrap_or(f64::NAN);
        let tail_s = tail_idx
            .map(|i| (num_sub_blocks - 1 - i) as f64 * block_dur_s)
            .unwrap_or(f64::NAN);
        println!(
            "  {:>4.0}  {:>14.2}  {:>10}  {:>12.2}  {:>12.2}",
            x,
            threshold,
            head_idx.map(|i| i as i64).unwrap_or(-1),
            head_s,
            tail_s
        );
        spacing_by_x.push((x as i32, head_s, tail_s));
    }
    let head_min = spacing_by_x.iter().map(|&(_, h, _)| h).fold(f64::MAX, f64::min);
    let head_max = spacing_by_x.iter().map(|&(_, h, _)| h).fold(f64::MIN, f64::max);
    let tail_min = spacing_by_x.iter().map(|&(_, _, t)| t).fold(f64::MAX, f64::min);
    let tail_max = spacing_by_x.iter().map(|&(_, _, t)| t).fold(f64::MIN, f64::max);
    println!(
        "\n  head_spacing εύρος στα X={{10..30}}: [{head_min:.2}, {head_max:.2}]s (spread {:.2}s)",
        head_max - head_min
    );
    println!(
        "  tail_spacing εύρος στα X={{10..30}}: [{tail_min:.2}, {tail_max:.2}]s (spread {:.2}s)",
        tail_max - tail_min
    );
    println!(
        "  ΠΛΑΤΟ αν το spread είναι μικρό σε σχέση με το εύρος 10-30dB του X (20dB) — \
         δηλ. αν {:.2}s ή {:.2}s << όσο θα έδινε γραμμική ευαισθησία."
        , head_max - head_min, tail_max - tail_min
    );

    // ─────────────────────────────────────────────────────────────
    // ΣΥΝΕΧΕΙΑ ΒΗΜΑΤΟΣ Γ — 3: ΜΕ ΤΟ ΜΑΤΙ — πρώτα/τελευταία 3s
    // ─────────────────────────────────────────────────────────────
    println!("\n=== 3. ΜΕ ΤΟ ΜΑΤΙ: πρώτα 3.0s (30 sub-blocks των 100ms) ===");
    for i in 0..30.min(num_sub_blocks) {
        println!(
            "  t={:>6.1}s  above_floor={:>7.2} dB",
            i as f64 * block_dur_s,
            above_floor[i]
        );
    }
    println!("\n=== 3. ΜΕ ΤΟ ΜΑΤΙ: τελευταία 3.0s (30 sub-blocks των 100ms) ===");
    let tail_start = num_sub_blocks.saturating_sub(30);
    for i in tail_start..num_sub_blocks {
        println!(
            "  t={:>6.1}s (t_end-{:>4.1}s)  above_floor={:>7.2} dB",
            i as f64 * block_dur_s,
            (num_sub_blocks - 1 - i) as f64 * block_dur_s,
            above_floor[i]
        );
    }
}
