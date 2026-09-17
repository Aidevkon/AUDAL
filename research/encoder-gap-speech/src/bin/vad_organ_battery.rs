//! MEASURE 2026-09-17: τα δύο ζωντανά όργανα φωνής (Phi2Sensor,
//! VadClassifier<FixedPriors>) στα δώδεκα δοκίμια + εννιά βιβλία.
//! Read-only. ΜΗΔΕΝ αλλαγή στην παραγωγή.
//!
//! ΒΗΜΑ 0.1: τα δύο posterior παίρνονται ΧΩΡΙΣ να στηθεί το NMF.
//! Όλα τα δομικά στοιχεία (VadFeatureExtractor, VadClassifier<
//! FixedPriors>, Phi2StreamingFrontend, Phi2Pcen, Phi2Sensor) είναι
//! δημόσια, αυτόνομα structs — ΔΕΝ χρειάζονται TwoPassEngine. Αυτό
//! το εργαλείο τα καλεί ΑΠΕΥΘΕΙΑΣ, ΙΔΙΑ σειρά με το production
//! (two_pass.rs:1656-1671,1785-1803· render_node.rs:1210+), αλλά
//! ΧΩΡΙΣ να αγγίξει render_node.rs/ducker/control_bus — μόνο
//! ανάγνωση των posterior, ΜΗΔΕΝ αλλαγή στην παραγωγή, ΜΗΔΕΝ
//! προσωρινό patch χρειάστηκε (η πύλη vad_observer.is_some() δεν
//! ζει στο ίδιο σημείο με το σήμα προς τον ducker — τα όργανα
//! καλούνται πριν από αυτό το σημείο).
//!
//! ΒΗΜΑ 0.2: τα όργανα δουλεύουν σε πλαίσια 10ms (FRAME_SAMPLES=480
//! @48kHz και για τα δύο — η ίδια ονομαστική ανάλυση, StftEngine
//! ξεχωριστό ανά όργανο). Το δοκίμιο έχει γνωστή απάντηση ανά
//! παράθυρο Scout 5s/hop 1s. Συνάθροιση: για κάθε παράθυρο, το
//! ΠΟΣΟΣΤΟ των πλαισίων 10ms μέσα σε αυτό με posterior > κατώφλι1,
//! συγκρίνεται με ΔΕΥΤΕΡΟ κατώφλι (ποσοστό πλαισίων). Δύο κατώφλια,
//! σάρωση και των δύο: posterior ∈ {0.3,0.5,0.7} × ποσοστό
//! ∈ {10%,30%,50%} = εννιά κελιά ανά όργανο. ΜΗΔΕΝ πρόταση τιμής.
//!
//! Ο κανόνας: όπου το ποσοστό ξεπερνάει το κατώφλι ⇒ Speech, αλλιώς
//! Music· ScoutDecision{leaning:1.0/0.0, confidence:1.0} ανά
//! παράθυρο (ΙΔΙΑ σύμβαση με τους Β/Γ/Δ κανόνες όλης της βάρδιας)
//! → smooth_and_segment (ο ΠΡΑΓΜΑΤΙΚΟΣ τμηματοποιητής).
//!
//! noise_floor_dbfs για το FixedPriors: quietest_active_window_dbfs
//! από το ΠΡΑΓΜΑΤΙΚΟ run_trunk_pass πάνω στο ΙΔΙΟ αρχείο — όχι
//! placeholder.
//!
//! ΧΡΗΣΗ: cargo run --release --bin vad_organ_battery

use lineos_corpus::scout::{smooth_and_segment, ScoutDecision, SegmentType};
use sp314_dsp::analysis::phi1_sensor::{Phi2Pcen, Phi2Sensor, Phi2StreamingFrontend};
use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{FixedPriors, VadClassifier};

const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const BOUNDARY_TOLERANCE_SECS: f32 = 5.0;
const DUMP_FRAME_BYTES: usize = 8;
const POST_THRESHOLDS: [f32; 3] = [0.3, 0.5, 0.7];
const FRAME_PCT_THRESHOLDS: [f32; 3] = [0.10, 0.30, 0.50];
const CHUNK: usize = 4096;

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

fn write_dump_stereo(path: &str, left: &[f32], right: &[f32]) {
    let mut buf = Vec::with_capacity(left.len() * DUMP_FRAME_BYTES);
    for i in 0..left.len() {
        buf.extend_from_slice(&left[i].to_le_bytes());
        buf.extend_from_slice(&right[i].to_le_bytes());
    }
    std::fs::write(path, &buf).unwrap_or_else(|e| panic!("write {path}: {e}"));
}

/// FixedPriors posterior ανά πλαίσιο 10ms — VadFeatureExtractor +
/// VadClassifier ΑΠΕΥΘΕΙΑΣ, ΧΩΡΙΣ TwoPassEngine.
fn fixed_priors_posteriors(mono: &[f32], left: &[f32], right: &[f32], noise_floor_dbfs: f32) -> Vec<f32> {
    let mut extractor = VadFeatureExtractor::new();
    let mut classifier = VadClassifier::new(FixedPriors);
    let features = extractor.process_chunk(mono, left, right);
    features.iter().map(|f| classifier.process(f, noise_floor_dbfs).posterior).collect()
}

/// Phi2Sensor posterior ανά πλαίσιο ~10ms (160 δείγματα @16kHz μετά
/// το decimate) — Phi2StreamingFrontend + Phi2Pcen + Phi2Sensor
/// ΑΠΕΥΘΕΙΑΣ, ΙΔΙΑ σειρά με two_pass.rs:1667-1671, ΧΩΡΙΣ NMF.
/// None (πρώτα 50 πλαίσια, ~0.5s warmup) παραλείπονται.
fn phi2_posteriors(mono_48k: &[f32]) -> Vec<f32> {
    let mut frontend = Phi2StreamingFrontend::new();
    let mut pcen = Phi2Pcen::new();
    let mut sensor = Phi2Sensor::new();
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < mono_48k.len() {
        let end = (pos + CHUNK).min(mono_48k.len());
        for mel in frontend.push(&mono_48k[pos..end]) {
            let p = pcen.process(&mel);
            if let Some(prob) = sensor.push_frame(&p) {
                out.push(prob);
            }
        }
        pos = end;
    }
    for mel in frontend.finish() {
        let p = pcen.process(&mel);
        if let Some(prob) = sensor.push_frame(&p) {
            out.push(prob);
        }
    }
    out
}

fn quietest_active_window_dbfs(dump_path: &str) -> f32 {
    let report = sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(dump_path), false)
        .unwrap_or_else(|e| panic!("run_trunk_pass({dump_path}): {e}"));
    report.metrics.quietest_active_window_dbfs.unwrap_or(-70.0)
}

fn expected_type(schedule: &[(f32, f32, SegmentType)], center: f32) -> SegmentType {
    for &(s, e, ty) in schedule {
        if center >= s && center < e { return ty; }
    }
    schedule.last().unwrap().2
}

fn expected_boundaries(schedule: &[(f32, f32, SegmentType)]) -> Vec<f32> {
    let mut out = Vec::new();
    for w in schedule.windows(2) {
        if w[0].2 != w[1].2 { out.push(w[1].0); }
    }
    out
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

fn music_to_speech_resets(segs: &[(f32, f32, SegmentType)]) -> usize {
    segs.windows(2).filter(|w| w[0].2 == SegmentType::Music && w[1].2 == SegmentType::Speech).count()
}

fn under_5s(segs: &[(f32, f32, SegmentType)]) -> usize {
    segs.iter().filter(|(s, e, _)| (e - s) < 5.0).count()
}

const A7_ZONE_LOW: f32 = 0.3;
const A7_ZONE_HIGH: f32 = 0.7;
const A7_MIN_CONF: f32 = 0.4;
fn flagged(leaning: f32, conf: f32) -> bool {
    (leaning > A7_ZONE_LOW && leaning < A7_ZONE_HIGH) && conf < A7_MIN_CONF
}

/// Window starts (5s/1s) για δεδομένο μήκος σήματος.
fn window_starts(n_samples: usize, sample_rate: u32) -> Vec<f32> {
    let win = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop = (HOP_SECS * sample_rate as f32) as usize;
    let mut out = Vec::new();
    let mut start = 0usize;
    while start + win <= n_samples {
        out.push(start as f32 / sample_rate as f32);
        start += hop;
    }
    out
}

/// Ποσοστό πλαισίων οργάνου μέσα σε [t, t+5s) με posterior > post_thr.
/// posts: (frame_time_sec, posterior), ΤΑΞΙΝΟΜΗΜΕΝΑ.
fn frame_fraction_above(posts: &[(f32, f32)], window_start: f32, post_thr: f32) -> f32 {
    let lo = posts.partition_point(|&(t, _)| t < window_start);
    let hi = posts.partition_point(|&(t, _)| t < window_start + WINDOW_SECS);
    if hi <= lo { return 0.0; }
    let total = hi - lo;
    let above = posts[lo..hi].iter().filter(|&&(_, p)| p > post_thr).count();
    above as f32 / total as f32
}

fn segments_from_decisions(times: &[f32], decisions: &[ScoutDecision]) -> Vec<(f32, f32, SegmentType)> {
    let d: Vec<(f32, ScoutDecision)> = times.iter().zip(decisions.iter()).map(|(&t, &dec)| (t, dec)).collect();
    smooth_and_segment(&d).iter().map(|b| (b.start_sec, b.end_sec, b.segment_type)).collect()
}

struct CellResult { correct: usize, total: usize, segs: Vec<(f32, f32, SegmentType)> }

fn cell_result(times: &[f32], posts: &[(f32, f32)], schedule: &[(f32, f32, SegmentType)], post_thr: f32, frame_pct_thr: f32) -> CellResult {
    let mut correct = 0usize;
    let mut decisions = Vec::with_capacity(times.len());
    for &t in times {
        let frac = frame_fraction_above(posts, t, post_thr);
        let verdict = if frac > frame_pct_thr { SegmentType::Speech } else { SegmentType::Music };
        let exp = expected_type(schedule, t + WINDOW_SECS / 2.0);
        if verdict == exp { correct += 1; }
        let leaning = if verdict == SegmentType::Speech { 1.0 } else { 0.0 };
        decisions.push(ScoutDecision { leaning_score: leaning, confidence: 1.0 });
    }
    let segs = segments_from_decisions(times, &decisions);
    CellResult { correct, total: times.len(), segs }
}

fn fmt_cell(label: &str, r: &CellResult, exp_bounds: &[f32]) -> String {
    let prod: Vec<f32> = r.segs.windows(2).map(|w| w[1].0).collect();
    let (f, m, e) = match_boundaries(exp_bounds, &prod, BOUNDARY_TOLERANCE_SECS);
    format!("{:<14} acc={:>3}/{} ({:>5.1}%)  όρια f={f}/{} m={m} e={e}", label, r.correct, r.total, 100.0 * r.correct as f32 / r.total as f32, exp_bounds.len())
}

struct Fixture { label: &'static str, wav: &'static str, schedule: Vec<(f32, f32, SegmentType)> }

fn run_books_only() {
    // ΗΔΗ ΜΕΤΡΗΜΕΝΑ (πλήρες τρέξιμο του ίδιου εργαλείου, τα δώδεκα δοκίμια,
    // κριτήριο φρουρός-πρώτα): Phi2Sensor post>0.5 frac>0.30, FixedPriors
    // post>0.3 frac>0.10. Επαναχρησιμοποιούνται εδώ ώστε τα εννιά βιβλία να
    // μην απαιτούν επανάληψη ολόκληρης της σάρωσης δώδεκα δοκιμίων.
    let ph_pt = 0.5_f32;
    let ph_ft = 0.30_f32;
    let fp_pt = 0.3_f32;
    let fp_ft = 0.10_f32;
    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;

    println!("=== ΤΑ ΕΝΝΙΑ ΒΙΒΛΙΑ, ΣΤΟ ΚΑΛΥΤΕΡΟ ΚΕΛΙ ΚΑΘΕ ΟΡΓΑΝΟΥ (books-only rerun) ===");
    println!("Phi2Sensor: post>{ph_pt:.1} frac>{ph_ft:.2}  ·  FixedPriors: post>{fp_pt:.1} frac>{fp_ft:.2}");
    println!("ΣΗΜΕΡΙΝΟ (Α): secretgarden=79 dracula=128 count_of_monte_cristo=16 τα άλλα 1-16. Επιπεδότητα (p10×1.75): secretgarden=77 dracula=128 count_of_monte_cristo=16.\n");

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
        let dump = format!("/tmp/vad_organ_book_{label}.raw");
        m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(&path), &dump)
            .unwrap_or_else(|e| panic!("decode {path}: {e}"));
        let (left, right) = read_dump_stereo(&dump);
        let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
        let noise_floor = quietest_active_window_dbfs(&dump);
        let _ = std::fs::remove_file(&dump);

        let times = window_starts(mono.len(), sample_rate);

        let fp_raw = fixed_priors_posteriors(&mono, &left, &right, noise_floor);
        let fp_posts: Vec<(f32, f32)> = fp_raw.iter().enumerate().map(|(i, &p)| (i as f32 * 480.0 / sample_rate as f32, p)).collect();
        let ph_raw = phi2_posteriors(&mono);
        let ph_posts: Vec<(f32, f32)> = ph_raw.iter().enumerate().map(|(i, &p)| ((i as f32 + 50.0) * 0.01, p)).collect();

        for (organ_label, posts, pt, ft) in [("Phi2", &ph_posts, ph_pt, ph_ft), ("FixedPriors", &fp_posts, fp_pt, fp_ft)] {
            let mut decisions = Vec::with_capacity(times.len());
            for &t in &times {
                let frac = frame_fraction_above(posts, t, pt);
                let verdict = if frac > ft { SegmentType::Speech } else { SegmentType::Music };
                let leaning = if verdict == SegmentType::Speech { 1.0 } else { 0.0 };
                decisions.push(ScoutDecision { leaning_score: leaning, confidence: 1.0 });
            }
            let segs = segments_from_decisions(&times, &decisions);
            let mut flagged_n = 0usize;
            for &(s, e, _) in &segs {
                let in_range: Vec<usize> = (0..times.len()).filter(|&i| times[i] >= s && times[i] < e).collect();
                if in_range.is_empty() { continue; }
                let avg_l = in_range.iter().map(|&i| decisions[i].leaning_score).sum::<f32>() / in_range.len() as f32;
                let avg_c = in_range.iter().map(|&i| decisions[i].confidence).sum::<f32>() / in_range.len() as f32;
                if flagged(avg_l, avg_c) { flagged_n += 1; }
            }
            println!(
                "{:<24} [{organ_label:<11}] τμήματα={:<4} resets(M→S)={:<4} flagged={:<4} <5s={}",
                label, segs.len(), music_to_speech_resets(&segs), flagged_n, under_5s(&segs)
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--books-only") {
        run_books_only();
        return;
    }
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

    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    println!("POST_THRESHOLDS={POST_THRESHOLDS:?} FRAME_PCT_THRESHOLDS={FRAME_PCT_THRESHOLDS:?} ΑΝΟΧΗ_ΟΡΙΩΝ={BOUNDARY_TOLERANCE_SECS}s\n");

    // Σάρωση: για κάθε δοκίμιο, υπολόγισε ΜΙΑ φορά τις ακολουθίες posterior
    // και τα window starts· ξαναχρησιμοποίησέ τα και στα εννέα κελιά ανά όργανο.
    struct FxData {
        times: Vec<f32>,
        fp_posts: Vec<(f32, f32)>,
        ph_posts: Vec<(f32, f32)>,
        cells_fp: Vec<Vec<(f32, f32, CellResult)>>,
        cells_ph: Vec<Vec<(f32, f32, CellResult)>>,
    }

    let mut data: Vec<FxData> = Vec::with_capacity(fixtures.len());
    for fx in &fixtures {
        let path = format!("/tmp/scout-groundtruth/{}", fx.wav);
        let (left, right) = read_wav_stereo_f32(&path);
        let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();

        let dump_path = format!("/tmp/vad_organ_{}.raw", fx.wav.trim_end_matches(".wav"));
        write_dump_stereo(&dump_path, &left, &right);
        let noise_floor = quietest_active_window_dbfs(&dump_path);
        let _ = std::fs::remove_file(&dump_path);

        let fp_raw = fixed_priors_posteriors(&mono, &left, &right, noise_floor);
        let fp_posts: Vec<(f32, f32)> = fp_raw.iter().enumerate().map(|(i, &p)| (i as f32 * 480.0 / sample_rate as f32, p)).collect();

        let ph_raw = phi2_posteriors(&mono);
        // Phi2: 10ms/frame @16k post-decimate, ΑΛΛΑ ο πρώτος posterior βγαίνει
        // μετά από PHI1_CONTEXT_FRAMES=51 πλαίσια context (~0.51s καθυστέρηση).
        let ph_posts: Vec<(f32, f32)> = ph_raw.iter().enumerate().map(|(i, &p)| ((i as f32 + 50.0) * 0.01, p)).collect();

        let times = window_starts(mono.len(), sample_rate);

        let mut cells_fp = Vec::with_capacity(3);
        let mut cells_ph = Vec::with_capacity(3);
        for &pt in &POST_THRESHOLDS {
            let mut row_fp = Vec::with_capacity(3);
            let mut row_ph = Vec::with_capacity(3);
            for &ft in &FRAME_PCT_THRESHOLDS {
                row_fp.push((pt, ft, cell_result(&times, &fp_posts, &fx.schedule, pt, ft)));
                row_ph.push((pt, ft, cell_result(&times, &ph_posts, &fx.schedule, pt, ft)));
            }
            cells_fp.push(row_fp);
            cells_ph.push(row_ph);
        }

        data.push(FxData { times, fp_posts, ph_posts, cells_fp, cells_ph });
    }

    let idx_of = |wav_prefix: &str| fixtures.iter().position(|f| f.wav.starts_with(wav_prefix)).unwrap();
    let idx4 = idx_of("test4");
    let idx10 = idx_of("test10");
    let idx2 = idx_of("test2");

    let print_fixture_block = |i: usize, fx: &Fixture| {
        let exp_bounds = expected_boundaries(&fx.schedule);
        println!("--- {} ---", fx.label);
        println!("  ΦΩΝΗ Phi2Sensor:");
        for row in &data[i].cells_ph {
            for (pt, ft, r) in row {
                println!("    post>{pt:.1} frac>{ft:.2}: {}", fmt_cell("Phi2", r, &exp_bounds));
            }
        }
        println!("  ΦΩΝΗ FixedPriors:");
        for row in &data[i].cells_fp {
            for (pt, ft, r) in row {
                println!("    post>{pt:.1} frac>{ft:.2}: {}", fmt_cell("FixedPriors", r, &exp_bounds));
            }
        }
    };

    println!("=== ΤΑ ΤΡΙΑ ΚΡΙΣΙΜΑ, ΠΡΩΤΑ ΚΑΙ ΞΕΧΩΡΙΣΤΑ ===");
    for &i in &[idx4, idx10, idx2] {
        print_fixture_block(i, &fixtures[i]);
    }

    println!("\n=== ΟΛΑ ΤΑ ΔΩΔΕΚΑ ===");
    for (i, fx) in fixtures.iter().enumerate() {
        print_fixture_block(i, fx);
    }

    // Επιλογή «καλύτερου κελιού» ανά όργανο: πρώτα ο φρουρός (δοκίμιο 2)
    // πρέπει να μείνει όσο γίνεται ψηλότερα· ισοπαλία σπάει με μέσο όρο
    // στα δώδεκα. ΜΕΤΡΗΜΕΝΗ επιλογή, όχι προτεινόμενη τιμή — τα εννιά
    // κελιά ήδη μετρήθηκαν παραπάνω.
    let cell_names: Vec<(f32, f32)> = POST_THRESHOLDS.iter().flat_map(|&p| FRAME_PCT_THRESHOLDS.iter().map(move |&f| (p, f))).collect();
    let pick_best = |fixture_idx_guard: usize, get_cells: &dyn Fn(&FxData) -> Vec<f32>| -> usize {
        let guard_accs: Vec<f32> = data.iter().map(|_| 0.0).collect();
        let _ = guard_accs;
        let per_cell_guard_acc = get_cells(&data[fixture_idx_guard]);
        let max_guard = per_cell_guard_acc.iter().cloned().fold(f32::MIN, f32::max);
        let candidates: Vec<usize> = (0..9).filter(|&ci| (per_cell_guard_acc[ci] - max_guard).abs() < 1e-6).collect();
        // ισοπαλία: μέσος όρος στα δώδεκα
        let mut best_ci = candidates[0];
        let mut best_avg = -1.0;
        for &ci in &candidates {
            let mut sum = 0.0;
            for fx_data in &data {
                let cells = get_cells(fx_data);
                sum += cells[ci];
            }
            let avg = sum / data.len() as f32;
            if avg > best_avg {
                best_avg = avg;
                best_ci = ci;
            }
        }
        best_ci
    };
    let ph_acc = |fx_data: &FxData| -> Vec<f32> {
        fx_data.cells_ph.iter().flat_map(|row| row.iter().map(|(_, _, r)| 100.0 * r.correct as f32 / r.total as f32)).collect()
    };
    let fp_acc = |fx_data: &FxData| -> Vec<f32> {
        fx_data.cells_fp.iter().flat_map(|row| row.iter().map(|(_, _, r)| 100.0 * r.correct as f32 / r.total as f32)).collect()
    };
    let best_ph_ci = pick_best(idx2, &ph_acc);
    let best_fp_ci = pick_best(idx2, &fp_acc);
    let (ph_pt, ph_ft) = cell_names[best_ph_ci];
    let (fp_pt, fp_ft) = cell_names[best_fp_ci];
    println!("\nΚαλύτερο κελί Phi2Sensor (φρουρός-πρώτα): post>{ph_pt:.1} frac>{ph_ft:.2}");
    println!("Καλύτερο κελί FixedPriors (φρουρός-πρώτα): post>{fp_pt:.1} frac>{fp_ft:.2}");

    println!("\n=== ΤΑ ΕΝΝΙΑ ΒΙΒΛΙΑ, ΣΤΟ ΚΑΛΥΤΕΡΟ ΚΕΛΙ ΚΑΘΕ ΟΡΓΑΝΟΥ ===");
    println!("ΣΗΜΕΡΙΝΟ (Α): secretgarden=79 dracula=128 count_of_monte_cristo=16 τα άλλα 1-16. Επιπεδότητα (p10×1.75): secretgarden=77 dracula=128 count_of_monte_cristo=16.\n");

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
        let dump = format!("/tmp/vad_organ_book_{label}.raw");
        m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(&path), &dump)
            .unwrap_or_else(|e| panic!("decode {path}: {e}"));
        let (left, right) = read_dump_stereo(&dump);
        let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
        let noise_floor = quietest_active_window_dbfs(&dump);
        let _ = std::fs::remove_file(&dump);

        let times = window_starts(mono.len(), sample_rate);

        let fp_raw = fixed_priors_posteriors(&mono, &left, &right, noise_floor);
        let fp_posts: Vec<(f32, f32)> = fp_raw.iter().enumerate().map(|(i, &p)| (i as f32 * 480.0 / sample_rate as f32, p)).collect();
        let ph_raw = phi2_posteriors(&mono);
        let ph_posts: Vec<(f32, f32)> = ph_raw.iter().enumerate().map(|(i, &p)| ((i as f32 + 50.0) * 0.01, p)).collect();

        for (organ_label, posts, pt, ft) in [("Phi2", &ph_posts, ph_pt, ph_ft), ("FixedPriors", &fp_posts, fp_pt, fp_ft)] {
            let mut decisions = Vec::with_capacity(times.len());
            for &t in &times {
                let frac = frame_fraction_above(posts, t, pt);
                let verdict = if frac > ft { SegmentType::Speech } else { SegmentType::Music };
                let leaning = if verdict == SegmentType::Speech { 1.0 } else { 0.0 };
                decisions.push(ScoutDecision { leaning_score: leaning, confidence: 1.0 });
            }
            let segs = segments_from_decisions(&times, &decisions);
            let mut flagged_n = 0usize;
            for &(s, e, _) in &segs {
                let in_range: Vec<usize> = (0..times.len()).filter(|&i| times[i] >= s && times[i] < e).collect();
                if in_range.is_empty() { continue; }
                let avg_l = in_range.iter().map(|&i| decisions[i].leaning_score).sum::<f32>() / in_range.len() as f32;
                let avg_c = in_range.iter().map(|&i| decisions[i].confidence).sum::<f32>() / in_range.len() as f32;
                if flagged(avg_l, avg_c) { flagged_n += 1; }
            }
            println!(
                "{:<24} [{organ_label:<11}] τμήματα={:<4} resets(M→S)={:<4} flagged={:<4} <5s={}",
                label, segs.len(), music_to_speech_resets(&segs), flagged_n, under_5s(&segs)
            );
        }
    }
}
