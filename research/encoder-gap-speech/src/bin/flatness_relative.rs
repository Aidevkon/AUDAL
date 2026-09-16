//! MEASURE 2026-09-16: η επιπεδότητα ως ΣΧΕΤΙΚΟ μέτρο, μέσα στο
//! αρχείο — Otsu στην κατανομή της ίδιας της επιπεδότητας ανά
//! παράθυρο, ΟΧΙ απόλυτο κατώφλι. Read-only, ΜΗΔΕΝ αλλαγή στην
//! παραγωγή/scout/centroids/κατώφλια/ανιχνευτή.
//!
//! Η ΜΕΘΟΔΟΣ, ΔΑΝΕΙΣΜΕΝΗ (ΟΧΙ Ο ΚΩΔΙΚΑΣ) από το quiet_window_split_dbfs
//! του expander (sp314-orchestrator/src/trunk_pass.rs:214-267,
//! otsu_split_bin + quiet_window_split_db): Otsu σε ιστόγραμμα,
//! μεγιστοποίηση της διακύμανσης ανάμεσα σε δύο κλάσεις (N. Otsu,
//! IEEE SMC 9, 1979). ΑΛΛΗ ποσότητα (επιπεδότητα [0,1] αντί dBFS),
//! ΑΛΛΟ εύρος/κάδοι (200 κάδοι πλάτους 0.005 στο [0,1] αντί 100
//! κάδων του 1dB σε [-100,0]) — η ΙΔΙΑ μαθηματική μέθοδος, νέος
//! κώδικας, καμία εξάρτηση από το trunk_pass.rs.
//!
//! ΜΕΤΡΟ ΔΙΤΡΟΠΙΑΣ (δηλωμένο, ΧΩΡΙΣ επιλογή κατωφλίου γι' αυτό):
//!   η² = σ²_between / σ²_total
//! όπου σ²_between = p0·p1·(m0-m1)² (Otsu το μεγιστοποιεί ήδη για να
//! διαλέξει την τομή) και σ²_total η διακύμανση ολόκληρου του
//! ιστογράμματος. Τυπικός δείκτης διαχωρισιμότητας Otsu — 0 σημαίνει
//! καμία διάκριση, 1 σημαίνει τέλειο διαχωρισμό. ΚΑΜΙΑ τιμή-κατώφλι
//! επιλέγεται εδώ — μόνο η κατανομή του αναφέρεται.
//!
//! ΑΝΑ ΠΑΡΑΘΥΡΟ 5s, η επιπεδότητα = η "εκδοχή Α" του F-115
//! (flatness_probe.rs): διάμεσος spectral_flatness() (μαύρο κουτί,
//! sp314-dsp/src/analysis/spectral.rs) πάνω σε 10 υπο-τμήματα των
//! 0.5s — ΙΔΙΟΣ ορισμός, ΜΗΔΕΝ νέα παραλλαγή.
//!
//! Ταξινόμηση: επιπεδότητα παραθύρου > κατώφλι(αρχείου) ⇒ Speech
//! (η φωνή έχει την ΨΗΛΟΤΕΡΗ επιπεδότητα, F-115).
//!
//! Όρια (μόνο για τα δώδεκα δοκίμια, όπου υπάρχει γνωστή απάντηση):
//! σύνθεση ScoutDecision{leaning:1.0/0.0, confidence:1.0} ανά
//! παράθυρο (ΙΔΙΑ σύμβαση με F-112/F-113/F-114) → smooth_and_segment
//! (ο τεμαχιστής) → σύγκριση με τα αναμενόμενα, ανοχή ±5.0s.
//!
//! Flagged (μόνο για τα εννιά βιβλία, όπου δεν υπάρχει γνωστή
//! απάντηση): ΤΟ ΠΡΑΓΜΑΤΙΚΟ avg leaning/confidence (compute_scout_
//! decision, F-041 κατώφλια), μέσος όρος πάνω στα παράθυρα του
//! τμήματος που παρήγαγε ΑΥΤΗ η ταξινόμηση — ΙΔΙΑ σύμβαση με το
//! flagged_count_from_real του F-112.
//!
//! Δοκίμια: τα δώδεκα του F-110/F-111. Βιβλία: τα εννιά του
//! librivox-hq. ΜΗΔΕΝ νέο υλικό.
//!
//! ΧΡΗΣΗ: cargo run --release --bin flatness_relative

use lineos_corpus::scout::{compute_cepstral_flux, compute_scout_decision, smooth_and_segment, ScoutDecision, SegmentType};
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

/// ΑΝΑ ΠΑΡΑΘΥΡΟ 5s: διάμεσος spectral_flatness() σε 10 υπο-τμήματα
/// 0.5s — ΙΔΙΟΣ ορισμός με το F-115 (flatness_probe.rs).
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

/// Otsu, ΙΔΙΑ μέθοδος με trunk_pass.rs:214-236 (otsu_split_bin) — νέος
/// κώδικας, νέο εύρος/κάδοι. Επιστρέφει (bin_index, between_class_var, total, sum_all).
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

/// Επιστρέφει (κατώφλι_επιπεδότητας, η²). ΠΑΝΤΑ κάποιο κατώφλι (Otsu
/// δεν ανιχνεύει διτροπία μόνος του) — το η² δηλώνεται δίπλα, ΧΩΡΙΣ
/// να κόβει τίποτα εδώ.
fn otsu_threshold_and_separability(values: &[f32]) -> (f32, f64) {
    let mut hist = [0u32; NBINS];
    for &v in values {
        let b = ((v / BIN_WIDTH) as usize).min(NBINS - 1);
        hist[b] += 1;
    }
    let total: f64 = hist.iter().map(|&c| c as f64).sum();
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let mean_total = sum_all / total;
    let var_total: f64 = hist.iter().enumerate().map(|(i, &c)| c as f64 * (i as f64 - mean_total).powi(2)).sum::<f64>() / total;

    match otsu_split_bin(&hist) {
        Some((t, best_var)) => {
            let sigma_b_sq = best_var / (total * total); // p0*p1*(m0-m1)^2
            let eta_sq = if var_total > 1e-12 { sigma_b_sq / var_total } else { 0.0 };
            let threshold = (t as f32 + 0.5) * BIN_WIDTH;
            (threshold, eta_sq)
        }
        None => (0.5, 0.0),
    }
}

fn flagged(leaning: f32, conf: f32) -> bool {
    let in_dead_zone = leaning > A7_ZONE_LOW && leaning < A7_ZONE_HIGH;
    let low_confidence = conf < A7_MIN_CONF;
    in_dead_zone && low_confidence
}

fn boundaries_from_verdicts(times: &[f32], verdicts: &[SegmentType]) -> Vec<(f32, f32, SegmentType)> {
    let decisions: Vec<(f32, ScoutDecision)> = times
        .iter()
        .zip(verdicts.iter())
        .map(|(&t, &v)| {
            let leaning = if v == SegmentType::Speech { 1.0 } else { 0.0 };
            (t, ScoutDecision { leaning_score: leaning, confidence: 1.0 })
        })
        .collect();
    smooth_and_segment(&decisions).iter().map(|b| (b.start_sec, b.end_sec, b.segment_type)).collect()
}

fn music_to_speech_resets(segs: &[(f32, f32, SegmentType)]) -> usize {
    segs.windows(2).filter(|w| w[0].2 == SegmentType::Music && w[1].2 == SegmentType::Speech).count()
}

fn under_5s(segs: &[(f32, f32, SegmentType)]) -> usize {
    segs.iter().filter(|(s, e, _)| (e - s) < 5.0).count()
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

struct Fixture {
    label: &'static str,
    wav: &'static str,
    schedule: Vec<(f32, f32, SegmentType)>,
}

fn run_fixture(fx: &Fixture) {
    let path = format!("/tmp/scout-groundtruth/{}", fx.wav);
    let (left, right) = read_wav_stereo_f32(&path);
    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

    let mut times = Vec::new();
    let mut flats = Vec::new();
    let mut start = 0usize;
    while start + win_samples <= mono.len() {
        let mono_slice = &mono[start..start + win_samples];
        let start_sec = start as f32 / sample_rate as f32;
        times.push(start_sec);
        flats.push(window_flatness(mono_slice, sample_rate));
        start += hop_samples;
    }

    let (thr, eta_sq) = otsu_threshold_and_separability(&flats);
    let mut correct = 0usize;
    let mut verdicts = Vec::new();
    for (i, &t) in times.iter().enumerate() {
        let center = t + WINDOW_SECS / 2.0;
        let exp = expected_type(&fx.schedule, center);
        let verdict = if flats[i] > thr { SegmentType::Speech } else { SegmentType::Music };
        if verdict == exp {
            correct += 1;
        }
        verdicts.push(verdict);
    }
    let total = times.len();
    let segs = boundaries_from_verdicts(&times, &verdicts);
    let exp_bounds = expected_boundaries(&fx.schedule);
    let prod_bounds: Vec<f32> = segs.windows(2).map(|w| w[1].0).collect();
    let (found, missed, extra) = match_boundaries(&exp_bounds, &prod_bounds, BOUNDARY_TOLERANCE_SECS);

    println!(
        "{:<24} thr={:.4}  η²={:.4}  ορθότητα={:>3}/{total} ({:>5.1}%)  όρια: βρέθηκαν={found}/{} χάθηκαν={missed} περίσσεψαν={extra}",
        fx.label, thr, eta_sq, correct, 100.0 * correct as f32 / total as f32, exp_bounds.len()
    );
}

fn run_book(label: &str, path: &str) {
    let dump = format!("/tmp/flatness_relative_{}.raw", label.split_whitespace().next().unwrap_or("x"));
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .unwrap_or_else(|e| panic!("pass0 decode {path}: {e}"));
    let (left, right) = read_dump_stereo(&dump);
    let _ = std::fs::remove_file(&dump);

    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

    let mut scout = SegmentScout::new();
    let mut times = Vec::new();
    let mut flats = Vec::new();
    let mut real_windows: Vec<(f32, f32, f32)> = Vec::new(); // (t, leaning, confidence) πραγματικά
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
        real_windows.push((start_sec, dec.leaning_score, dec.confidence));

        times.push(start_sec);
        flats.push(window_flatness(mono_slice, sample_rate));
        start += hop_samples;
    }

    let (thr, eta_sq) = otsu_threshold_and_separability(&flats);
    let verdicts: Vec<SegmentType> = flats.iter().map(|&f| if f > thr { SegmentType::Speech } else { SegmentType::Music }).collect();
    let segs = boundaries_from_verdicts(&times, &verdicts);

    let mut flagged_n = 0usize;
    for &(s, e, _) in &segs {
        let in_range: Vec<&(f32, f32, f32)> = real_windows.iter().filter(|(t, _, _)| *t >= s && *t < e).collect();
        if in_range.is_empty() {
            continue;
        }
        let avg_l = in_range.iter().map(|(_, l, _)| l).sum::<f32>() / in_range.len() as f32;
        let avg_c = in_range.iter().map(|(_, _, c)| c).sum::<f32>() / in_range.len() as f32;
        if flagged(avg_l, avg_c) {
            flagged_n += 1;
        }
    }

    println!(
        "{:<24} thr={:.4}  η²={:.4}  τμήματα={:<4} resets(M→S)={:<4} flagged={:<4} <5s={}",
        label, thr, eta_sq, segs.len(), music_to_speech_resets(&segs), flagged_n, under_5s(&segs)
    );
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

    println!("NBINS={NBINS} BIN_WIDTH={BIN_WIDTH:.5} ΑΝΟΧΗ_ΟΡΙΩΝ={BOUNDARY_TOLERANCE_SECS}s\n");
    println!("=== ΤΑ ΔΩΔΕΚΑ ΔΟΚΙΜΙΑ ===");
    // ΤΟ (10) ΠΡΩΤΟ
    for fx in fixtures.iter().filter(|f| f.wav.starts_with("test10")) {
        run_fixture(fx);
    }
    for fx in fixtures.iter().filter(|f| !f.wav.starts_with("test10")) {
        run_fixture(fx);
    }

    println!("\n=== ΤΑ ΕΝΝΙΑ ΒΙΒΛΙΑ ===");
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
