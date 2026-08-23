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

    let tmp = PathBuf::from("/tmp/spacing_distribution_decode.pcm");
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
    let input = "/home/aidevcon/Downloads/DATASET/24 - The Wise In The Desert.flac";
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

    println!("\n=== ΣΥΝΟΨΗ ===");
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
}
