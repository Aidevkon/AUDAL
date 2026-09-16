//! MEASURE 2026-09-16: ο ασύμμετρος κανόνας της επιπεδότητας.
//! Read-only, ΜΗΔΕΝ αλλαγή στην παραγωγή.
//!
//! Ο ΚΑΝΟΝΑΣ (πρόταση ιδιοκτήτη, 16/09), ανά παράθυρο:
//!   αν η σημερινή προβολή (compute_scout_decision, F-041) λέει
//!   Speech (leaning>=0.5) ⇒ ΚΡΑΤΑΕΙ ΩΣ ΕΧΕΙ (ίδιο leaning, ίδιο
//!   confidence). Αν λέει Music ⇒ κοιτάει την επιπεδότητα του
//!   παραθύρου (ΙΔΙΟΣ ορισμός με F-115/flatness_relative.rs: διάμεσος
//!   spectral_flatness() σε 10 υπο-τμήματα 0.5s): κάτω από το
//!   κατώφλι ⇒ μένει Music (leaning/confidence αναλλοίωτα)· πάνω ⇒
//!   γίνεται Speech με confidence override (η πρόταση: 0.80).
//!   Μονόδρομος: ποτέ δεν μετατρέπει Speech σε Music.
//!
//! Οι πραγματικές (leaning, confidence) τιμές —όχι συνθετικές 1.0—
//! τροφοδοτούν το ΠΡΑΓΜΑΤΙΚΟ smooth_and_segment, ώστε ο μηχανισμός
//! confidence-as-EMA-step (F-107) να λειτουργεί όπως στην παραγωγή.
//!
//! ΤΕΣΣΕΡΑ ΣΚΕΛΗ:
//!   1. Ο κανόνας όπως προτάθηκε (thr=0.045 απόλυτο, conf=0.80) στα
//!      δώδεκα δοκίμια — ορθότητα + όρια (ανοχή ±5s), δίπλα η
//!      σημερινή προβολή (Α) και ο εκφυλισμένος (Δ). Δοκίμιο 10 πρώτο.
//!   2. Σάρωση κατωφλίου: 0.035/0.040/0.045/0.050/0.060, ίδιο conf.
//!   3. Το ΣΧΕΤΙΚΟ (Otsu στην κατανομή επιπεδότητας του ΙΔΙΟΥ
//!      αρχείου — ΙΔΙΑ μέθοδος με flatness_relative.rs) δίπλα στο
//!      απόλυτο 0.045, ίδιο conf.
//!   4. Η ΠΥΛΗ: τα εννιά πραγματικά βιβλία, σχετικό κατώφλι
//!      (parameter-free ανά αρχείο), conf ∈ {0.50, 0.80, 0.95}.
//!      Ίδιος αναγνώστης με real_files_battery.rs/flatness_relative.rs
//!      (m0d::dsp::input_lufs::pass0_decode_to_dump).
//!
//! ΜΗΔΕΝ πρόταση τιμής εδώ — 0.045/0.80 είναι η ΠΡΟΤΑΣΗ του task,
//! μετρημένη, όχι προτεινόμενη από αυτό το εργαλείο.
//!
//! ΧΡΗΣΗ: cargo run --release --bin flatness_asymmetric_battery

use lineos_corpus::scout::{
    compute_cepstral_flux, compute_scout_decision, smooth_and_segment, ScoutDecision, SegmentType,
};
use sp314_dsp::analysis::scout::SegmentScout;
use sp314_dsp::analysis::spectral::spectral_flatness;

const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const SUBCHUNK_SECS: f32 = 0.5;
const NBINS: usize = 200;
const BIN_WIDTH: f32 = 1.0 / NBINS as f32;
const BOUNDARY_TOLERANCE_SECS: f32 = 5.0;
const A7_ZONE_LOW: f32 = 0.3;
const A7_ZONE_HIGH: f32 = 0.7;
const A7_MIN_CONF: f32 = 0.4;
const DUMP_FRAME_BYTES: usize = 8;

const OWNER_THRESHOLD: f32 = 0.045;
const OWNER_CONF: f32 = 0.80;
const SWEEP_THRESHOLDS: [f32; 5] = [0.035, 0.040, 0.045, 0.050, 0.060];
const CONF_SWEEP: [f32; 3] = [0.50, 0.80, 0.95];

fn read_wav_stereo_f32(path: &str) -> (Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let samples: Vec<f32> = reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
    let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
    (left, right)
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

fn median(xs: &[f32]) -> f32 {
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n == 0 {
        return f32::NAN;
    }
    if n % 2 == 0 { (v[n / 2 - 1] + v[n / 2]) / 2.0 } else { v[n / 2] }
}

/// ΙΔΙΟΣ ορισμός με F-115/flatness_relative.rs: διάμεσος spectral_flatness()
/// (μαύρο κουτί, sp314-dsp/src/analysis/spectral.rs) σε 10 υπο-τμήματα 0.5s.
fn window_flatness(mono_slice: &[f32], sample_rate: u32) -> f32 {
    let sub_samples = (SUBCHUNK_SECS * sample_rate as f32) as usize;
    let mut vals = Vec::new();
    let mut pos = 0;
    while pos + sub_samples <= mono_slice.len() {
        vals.push(spectral_flatness(&mono_slice[pos..pos + sub_samples]));
        pos += sub_samples;
    }
    median(&vals)
}

/// ΙΔΙΑ μέθοδος (Otsu) με flatness_relative.rs — νέος κώδικας, ίδια
/// μαθηματική βάση με trunk_pass.rs:214-236 (otsu_split_bin).
fn otsu_split_bin(hist: &[u32; NBINS]) -> Option<(usize, f64)> {
    let total: f64 = hist.iter().map(|&c| c as f64).sum();
    if total == 0.0 {
        return None;
    }
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut w0, mut sum0) = (0.0_f64, 0.0_f64);
    let (mut best_var, mut best_t) = (-1.0_f64, 0usize);
    for t in 0..NBINS {
        w0 += hist[t] as f64;
        if w0 == 0.0 {
            continue;
        }
        let w1 = total - w0;
        if w1 == 0.0 {
            break;
        }
        sum0 += t as f64 * hist[t] as f64;
        let m0 = sum0 / w0;
        let m1 = (sum_all - sum0) / w1;
        let var = w0 * w1 * (m0 - m1) * (m0 - m1);
        if var > best_var {
            best_var = var;
            best_t = t;
        }
    }
    if best_var < 0.0 {
        None
    } else {
        Some((best_t, best_var))
    }
}

fn otsu_threshold(values: &[f32]) -> f32 {
    let mut hist = [0u32; NBINS];
    for &v in values {
        let b = ((v / BIN_WIDTH) as usize).min(NBINS - 1);
        hist[b] += 1;
    }
    match otsu_split_bin(&hist) {
        Some((t, _)) => (t as f32 + 0.5) * BIN_WIDTH,
        None => 0.5,
    }
}

/// Ο ασύμμετρος κανόνας. Μονόδρομος: Speech της προβολής ΠΟΤΕ δεν αλλάζει.
fn apply_asymmetric(dec_a: ScoutDecision, flatness: f32, threshold: f32, override_conf: f32) -> ScoutDecision {
    if dec_a.leaning_score >= 0.5 {
        dec_a
    } else if flatness > threshold {
        ScoutDecision { leaning_score: 1.0, confidence: override_conf }
    } else {
        dec_a
    }
}

fn match_boundaries(expected: &[f32], produced: &[f32], tol: f32) -> (usize, usize, usize) {
    let mut used = vec![false; produced.len()];
    let mut found = 0usize;
    for &e in expected {
        if let Some(idx) = produced
            .iter()
            .enumerate()
            .filter(|(i, &p)| !used[*i] && (p - e).abs() <= tol)
            .min_by(|a, b| (a.1 - e).abs().partial_cmp(&(b.1 - e).abs()).unwrap())
            .map(|(i, _)| i)
        {
            used[idx] = true;
            found += 1;
        }
    }
    let missed = expected.len() - found;
    let extra = used.iter().filter(|&&u| !u).count();
    (found, missed, extra)
}

fn expected_type(schedule: &[(f32, f32, SegmentType)], center: f32) -> SegmentType {
    for &(s, e, ty) in schedule {
        if center >= s && center < e {
            return ty;
        }
    }
    schedule.last().unwrap().2
}

fn expected_boundaries(schedule: &[(f32, f32, SegmentType)]) -> Vec<f32> {
    let mut out = Vec::new();
    for w in schedule.windows(2) {
        if w[0].2 != w[1].2 {
            out.push(w[1].0);
        }
    }
    out
}

fn music_to_speech_resets(segs: &[(f32, f32, SegmentType)]) -> usize {
    segs.windows(2).filter(|w| w[0].2 == SegmentType::Music && w[1].2 == SegmentType::Speech).count()
}

fn under_5s(segs: &[(f32, f32, SegmentType)]) -> usize {
    segs.iter().filter(|(s, e, _)| (e - s) < 5.0).count()
}

fn flagged(leaning: f32, conf: f32) -> bool {
    let in_dead_zone = leaning > A7_ZONE_LOW && leaning < A7_ZONE_HIGH;
    let low_confidence = conf < A7_MIN_CONF;
    in_dead_zone && low_confidence
}

struct Fixture {
    label: &'static str,
    wav: &'static str,
    schedule: Vec<(f32, f32, SegmentType)>,
}

struct WindowData {
    times: Vec<f32>,
    dec_a: Vec<ScoutDecision>,
    flats: Vec<f32>,
}

fn scan_fixture(fx: &Fixture) -> WindowData {
    let path = format!("/tmp/scout-groundtruth/{}", fx.wav);
    let (left, right) = read_wav_stereo_f32(&path);
    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    scan_mono(&mono, sample_rate)
}

fn scan_mono(mono: &[f32], sample_rate: u32) -> WindowData {
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;
    let mut scout = SegmentScout::new();
    let mut times = Vec::new();
    let mut dec_a = Vec::new();
    let mut flats = Vec::new();
    let mut start = 0usize;
    while start + win_samples <= mono.len() {
        let mono_slice = &mono[start..start + win_samples];
        let start_sec = start as f32 / sample_rate as f32;

        let mut mfcc_analyzer = lineos_corpus::mfcc::MfccAnalyzer::new();
        let mut mfccs = Vec::new();
        let mut f = 0;
        while f + 1024 <= mono_slice.len() {
            mfccs.push(mfcc_analyzer.compute(&mono_slice[f..f + 1024]));
            f += 512;
        }
        let cepstral_flux = compute_cepstral_flux(&mfccs);
        let meas = scout.measure(mono_slice, cepstral_flux, sample_rate);
        let dec = compute_scout_decision(&meas);

        times.push(start_sec);
        dec_a.push(dec);
        flats.push(window_flatness(mono_slice, sample_rate));
        start += hop_samples;
    }
    WindowData { times, dec_a, flats }
}

fn segments_from_decisions(times: &[f32], decisions: &[ScoutDecision]) -> Vec<(f32, f32, SegmentType)> {
    let d: Vec<(f32, ScoutDecision)> = times.iter().zip(decisions.iter()).map(|(&t, &dec)| (t, dec)).collect();
    smooth_and_segment(&d).iter().map(|b| (b.start_sec, b.end_sec, b.segment_type)).collect()
}

fn fixture_result(
    wd: &WindowData,
    schedule: &[(f32, f32, SegmentType)],
    threshold: f32,
    override_conf: f32,
) -> (usize, usize, Vec<(f32, f32, SegmentType)>) {
    let total = wd.times.len();
    let mut correct = 0usize;
    let mut final_decisions = Vec::with_capacity(total);
    for i in 0..total {
        let center = wd.times[i] + WINDOW_SECS / 2.0;
        let exp = expected_type(schedule, center);
        let final_dec = apply_asymmetric(wd.dec_a[i], wd.flats[i], threshold, override_conf);
        let verdict = if final_dec.leaning_score >= 0.5 { SegmentType::Speech } else { SegmentType::Music };
        if verdict == exp {
            correct += 1;
        }
        final_decisions.push(final_dec);
    }
    let segs = segments_from_decisions(&wd.times, &final_decisions);
    (correct, total, segs)
}

fn today_a_delta(wd: &WindowData, schedule: &[(f32, f32, SegmentType)]) -> (usize, usize, Vec<(f32, f32, SegmentType)>) {
    let total = wd.times.len();
    let mut correct_a = 0usize;
    for i in 0..total {
        let center = wd.times[i] + WINDOW_SECS / 2.0;
        let exp = expected_type(schedule, center);
        let v_a = if wd.dec_a[i].leaning_score >= 0.5 { SegmentType::Speech } else { SegmentType::Music };
        if v_a == exp {
            correct_a += 1;
        }
    }
    let segs_a = segments_from_decisions(&wd.times, &wd.dec_a);
    (correct_a, total, segs_a)
}

fn print_row(label: &str, correct: usize, total: usize, segs: &[(f32, f32, SegmentType)], exp_bounds: &[f32]) {
    let prod: Vec<f32> = segs.windows(2).map(|w| w[1].0).collect();
    let (found, missed, extra) = match_boundaries(exp_bounds, &prod, BOUNDARY_TOLERANCE_SECS);
    println!(
        "  {:<14} ορθότητα={:>3}/{total} ({:>5.1}%)  όρια: βρέθηκαν={found}/{} χάθηκαν={missed} περίσσεψαν={extra}",
        label, correct, 100.0 * correct as f32 / total as f32, exp_bounds.len()
    );
}

fn run_book(label: &str, path: &str) {
    let dump = format!("/tmp/flatness_asym_{}.raw", label.split_whitespace().next().unwrap_or("x"));
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .unwrap_or_else(|e| panic!("pass0 decode {path}: {e}"));
    let (left, right) = read_dump_stereo(&dump);
    let _ = std::fs::remove_file(&dump);
    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    let wd = scan_mono(&mono, sample_rate);
    let thr = otsu_threshold(&wd.flats);

    print!("{:<24} thr(Otsu)={:.4}", label, thr);
    for &conf in &CONF_SWEEP {
        let final_decisions: Vec<ScoutDecision> = (0..wd.times.len())
            .map(|i| apply_asymmetric(wd.dec_a[i], wd.flats[i], thr, conf))
            .collect();
        let segs = segments_from_decisions(&wd.times, &final_decisions);
        let mut flagged_n = 0usize;
        for &(s, e, _) in &segs {
            let in_range: Vec<usize> = (0..wd.times.len()).filter(|&i| wd.times[i] >= s && wd.times[i] < e).collect();
            if in_range.is_empty() {
                continue;
            }
            let avg_l = in_range.iter().map(|&i| final_decisions[i].leaning_score).sum::<f32>() / in_range.len() as f32;
            let avg_c = in_range.iter().map(|&i| final_decisions[i].confidence).sum::<f32>() / in_range.len() as f32;
            if flagged(avg_l, avg_c) {
                flagged_n += 1;
            }
        }
        print!(
            "   [conf={:.2}] τμήματα={:<4} resets(M→S)={:<4} flagged={:<4} <5s={}",
            conf, segs.len(), music_to_speech_resets(&segs), flagged_n, under_5s(&segs)
        );
    }
    println!();
}

fn main() {
    use SegmentType::{Music, Speech};
    let fixtures = vec![
        Fixture { label: "1 ΣΚΕΤΗ ΑΦΗΓΗΣΗ", wav: "test1_narration_only.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "2 ΣΚΕΤΟ BED", wav: "test2_bed_only.wav", schedule: vec![(0.0, 120.0, Music)] },
        Fixture { label: "3 ΑΦΗΓΗΣΗ+BED -20dB", wav: "test3_narration_bed_m20dB.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "4 ΑΦΗΓΗΣΗ+BED -12dB", wav: "test4_narration_bed_m12dB.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "5 ΕΝΑΛΛΑΓΗ 30/60/90", wav: "test5_alternation_30_60_90.wav", schedule: vec![(0.0, 30.0, Speech), (30.0, 60.0, Music), (60.0, 90.0, Speech), (90.0, 120.0, Music)] },
        Fixture { label: "6 BED ΜΠΑΙΝΕΙ ΣΤΑ 60s", wav: "test6_bed_enters_60s.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "7 ΜΗ-ΣΤΡΟΓΓΥΛΕΣ", wav: "test7_nonround_17_41_68_94.wav", schedule: vec![(0.0, 17.0, Speech), (17.0, 41.0, Music), (41.0, 68.0, Speech), (68.0, 94.0, Music), (94.0, 120.0, Speech)] },
        Fixture { label: "8 ΣΥΝΤΟΜΕΣ 20s×6", wav: "test8_short_20s_x6.wav", schedule: vec![(0.0, 20.0, Speech), (20.0, 40.0, Music), (40.0, 60.0, Speech), (60.0, 80.0, Music), (80.0, 100.0, Speech), (100.0, 120.0, Music)] },
        Fixture { label: "9 ΑΣΥΜΜΕΤΡΕΣ", wav: "test9_asymmetric.wav", schedule: vec![(0.0, 45.0, Speech), (45.0, 53.0, Music), (53.0, 103.0, Speech), (103.0, 111.0, Music), (111.0, 120.0, Speech)] },
        Fixture { label: "10 BED ΣΥΝΕΧΙΖΕΙ", wav: "test10_bed_continues_under.wav", schedule: vec![(0.0, 40.0, Speech), (40.0, 60.0, Music), (60.0, 120.0, Speech)] },
        Fixture { label: "11 ΑΛΛΟ BED", wav: "test11_altbed_17_41_68_94.wav", schedule: vec![(0.0, 17.0, Speech), (17.0, 41.0, Music), (41.0, 68.0, Speech), (68.0, 94.0, Music), (94.0, 120.0, Speech)] },
        Fixture { label: "12 ΑΛΛΟΣ ΑΦΗΓΗΤΗΣ", wav: "test12_altnarrator_17_41_68_94.wav", schedule: vec![(0.0, 17.0, Speech), (17.0, 41.0, Music), (41.0, 68.0, Speech), (68.0, 94.0, Music), (94.0, 120.0, Speech)] },
    ];

    // Scan once per fixture, reuse across all legs.
    let scans: Vec<WindowData> = fixtures.iter().map(scan_fixture).collect();

    println!("OWNER_THRESHOLD={OWNER_THRESHOLD} OWNER_CONF={OWNER_CONF} ΑΝΟΧΗ_ΟΡΙΩΝ={BOUNDARY_TOLERANCE_SECS}s\n");

    println!("=== ΣΚΕΛΟΣ 1 — Ο ΚΑΝΟΝΑΣ ΟΠΩΣ ΠΡΟΤΑΘΗΚΕ (thr=0.045 απόλυτο, conf=0.80) ===");
    let order: Vec<usize> = {
        let mut idx10 = None;
        let mut rest = Vec::new();
        for (i, fx) in fixtures.iter().enumerate() {
            if fx.wav.starts_with("test10") { idx10 = Some(i); } else { rest.push(i); }
        }
        let mut v = vec![idx10.unwrap()];
        v.extend(rest);
        v
    };
    for &i in &order {
        let fx = &fixtures[i];
        let wd = &scans[i];
        let exp_bounds = expected_boundaries(&fx.schedule);
        println!("--- {} ---", fx.label);
        let (c_a, t_a, s_a) = today_a_delta(wd, &fx.schedule);
        print_row("Α (σήμερα)", c_a, t_a, &s_a, &exp_bounds);
        let (c_r, t_r, s_r) = fixture_result(wd, &fx.schedule, OWNER_THRESHOLD, OWNER_CONF);
        print_row("ασύμμετρος", c_r, t_r, &s_r, &exp_bounds);
        let always_speech: Vec<ScoutDecision> = wd.times.iter().map(|_| ScoutDecision { leaning_score: 1.0, confidence: 1.0 }).collect();
        let segs_d = segments_from_decisions(&wd.times, &always_speech);
        let c_d = (0..wd.times.len()).filter(|&i2| expected_type(&fx.schedule, wd.times[i2] + WINDOW_SECS / 2.0) == Speech).count();
        print_row("Δ (πάντα ομιλία)", c_d, wd.times.len(), &segs_d, &exp_bounds);
    }

    println!("\n=== ΣΚΕΛΟΣ 2 — ΣΑΡΩΣΗ ΚΑΤΩΦΛΙΟΥ (conf=0.80, απόλυτο) ===");
    print!("{:<24}", "δοκίμιο");
    for t in SWEEP_THRESHOLDS {
        print!("  thr={:.3}", t);
    }
    println!();
    for (i, fx) in fixtures.iter().enumerate() {
        let wd = &scans[i];
        print!("{:<24}", fx.label);
        for &t in &SWEEP_THRESHOLDS {
            let (c, tot, _) = fixture_result(wd, &fx.schedule, t, OWNER_CONF);
            print!("  {:>6.1}%", 100.0 * c as f32 / tot as f32);
        }
        println!();
    }

    println!("\n=== ΣΚΕΛΟΣ 3 — ΣΧΕΤΙΚΟ (Otsu ανά αρχείο) έναντι ΑΠΟΛΥΤΟ 0.045, conf=0.80 ===");
    for (i, fx) in fixtures.iter().enumerate() {
        let wd = &scans[i];
        let exp_bounds = expected_boundaries(&fx.schedule);
        let rel_thr = otsu_threshold(&wd.flats);
        println!("--- {} (Otsu thr={:.4}) ---", fx.label, rel_thr);
        let (c_abs, t_abs, s_abs) = fixture_result(wd, &fx.schedule, OWNER_THRESHOLD, OWNER_CONF);
        print_row("απόλυτο 0.045", c_abs, t_abs, &s_abs, &exp_bounds);
        let (c_rel, t_rel, s_rel) = fixture_result(wd, &fx.schedule, rel_thr, OWNER_CONF);
        print_row("σχετικό (Otsu)", c_rel, t_rel, &s_rel, &exp_bounds);
    }

    println!("\n=== ΣΚΕΛΟΣ 4 — Η ΠΥΛΗ: ΕΝΝΙΑ ΒΙΒΛΙΑ, ΣΧΕΤΙΚΟ ΚΑΤΩΦΛΙ, conf ∈ {{0.50,0.80,0.95}} ===");
    println!("ΣΗΜΕΡΙΝΟ (F-107/F-112, Α — για σύγκριση, ΗΔΗ μετρημένο σε docs/lab-logs/real-files-battery-20260916.txt):");
    println!("  secretgarden  79/39/19/45   dracula 128/63/27/59   τα επτά άλλα 1-16 τμήματα   flux-μόνο(Β): secretgarden=141 dracula=397\n");
    let dir = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    let books = [
        ("secretgarden", "secretgarden_01_burnett.mp3"),
        ("dracula", "dracula_01_stoker.mp3"),
        ("count_of_monte_cristo", "count_of_monte_cristo_001_dumas.mp3"),
        ("peterpan", "peterpan_01_barrie.mp3"),
        ("anne_of_green_gables", "anne_of_green_gables_01_montgomery.mp3"),
        ("tale_of_two_cities", "tale_of_two_cities_01_dickens.mp3"),
        ("adventurespinocchio", "adventurespinocchio_01_collodi.mp3"),
        ("huckfinn", "huckfinn_01_twain_apc.mp3"),
        ("janeeyre", "janeeyre_01_bronte.mp3"),
    ];
    for (label, fname) in books {
        let path = format!("{dir}/{fname}");
        run_book(label, &path);
    }
}
