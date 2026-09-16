//! MEASURE 2026-09-16: τα δώδεκα δοκίμια του F-109/F-110/fixture_battery,
//! ξανά, με τον ΚΥΛΙΟΜΕΝΟ ορισμό του cepstral_flux — αυτόν που τρέχει η
//! παραγωγή (F-111). Read-only, ΜΗΔΕΝ αλλαγή στην παραγωγή.
//!
//! ΒΗΜΑ 0, απάντηση: ΝΑΙ, καλείται το ΠΡΑΓΜΑΤΙΚΟ
//! sp314_orchestrator::trunk_pass::run_trunk_pass απευθείας — καμία
//! αναπαραγωγή λογικής. Το μόνο νέο βήμα είναι μετατροπή ΜΟΡΦΗΣ: τα wav
//! δοκίμια (i16, 48k, stereo — ίδιος reader με το fixture_battery.rs) εδώ
//! γράφονται ΚΑΙ ως headerless interleaved f32 LE dump (η μορφή που
//! περιμένει RawPcmFileSource — sp314_dsp/src/stft/raw_pcm_source.rs,
//! ίδια σύμβαση με decode_provider.rs's DumpSeekProvider), χρησιμοποιώντας
//! ΤΗΝ ΙΔΙΑ μετατροπή i16→f32 (/32768.0) που ήδη χρησιμοποιεί το
//! fixture_battery.rs. Καμία λογική DSP δεν ξαναγράφεται εδώ — μόνο
//! bytes-σε-δίσκο.
//!
//! ΠΙΣΤΟΤΗΤΑ ΤΗΣ ΜΕΤΑΤΡΟΠΗΣ, ελεγμένη ΑΝΑ ΔΟΚΙΜΙΟ πριν εμπιστευτεί
//! οποιοδήποτε νούμερο: (1) το πλήθος παραθύρων από το πραγματικό
//! TrunkReport.metrics.cv_ioi_sequence πρέπει να ταυτίζεται με το πλήθος
//! του φρέσκου περάσματος· (2) το F-111 μέτρησε cv_ioi ταυτόσημο ανάμεσα
//! στους δύο ορισμούς σε πραγματικά αρχεία — άρα max|cv_ioi_φρέσκο -
//! cv_ioi_κυλιόμενο| πρέπει να είναι ίδιας τάξης (<0.0001) εδώ. Αν όχι, η
//! μετατροπή dump είναι λάθος και η μέτρηση ΔΕΝ γράφεται.
//!
//! ΙΔΙΕΣ σταθερές/κανόνες με το fixture_battery.rs: MUSIC_CV/SPEECH_CV/
//! MUSIC_FLUX/SPEECH_FLUX (F-041), MID_CV/MID_FLUX, ΑΝΟΧΗ ορίων ±5.0s,
//! ΙΔΙΑ δώδεκα δοκίμια/schedules, ΙΔΙΟ smooth_and_segment.
//!
//! ΧΡΗΣΗ: cargo run --release --bin rolling_battery

use lineos_corpus::scout::{
    compute_cepstral_flux, compute_scout_decision, smooth_and_segment, ScoutDecision, ScoutMeasurements,
    SegmentType,
};
use sp314_dsp::analysis::scout::SegmentScout;
use sp314_orchestrator::trunk_pass::run_trunk_pass;
use std::path::Path;

const WORK: &str = "/tmp/scout-groundtruth";
const DUMP_DIR: &str = "/tmp/rolling-battery-dumps";
const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const BOUNDARY_TOLERANCE_SECS: f32 = 5.0;

const MUSIC_CV: f32 = 0.4375;
const SPEECH_CV: f32 = 0.6855;
const MUSIC_FLUX: f32 = 1.3387;
const SPEECH_FLUX: f32 = 1.7242;
const MID_CV: f32 = (MUSIC_CV + SPEECH_CV) / 2.0;
const MID_FLUX: f32 = (MUSIC_FLUX + SPEECH_FLUX) / 2.0;

fn read_wav_stereo_f32(path: &str) -> (Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let samples: Vec<f32> = reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
    let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
    (left, right)
}

/// ΜΟΝΟ μετατροπή μορφής: headerless interleaved f32 LE, ίδια i16→f32
/// σύμβαση (/32768.0) με το fixture_battery.rs. ΔΕΝ αγγίζει DSP λογική.
fn wav_to_raw_dump(wav_path: &str, dump_path: &Path) {
    let mut reader = hound::WavReader::open(wav_path).unwrap_or_else(|e| panic!("open {wav_path}: {e}"));
    let mut buf: Vec<u8> = Vec::new();
    for s in reader.samples::<i16>() {
        let f = s.unwrap() as f32 / 32768.0;
        buf.extend_from_slice(&f.to_le_bytes());
    }
    std::fs::write(dump_path, &buf).unwrap_or_else(|e| panic!("write {}: {e}", dump_path.display()));
}

struct Fixture {
    label: &'static str,
    wav: &'static str,
    schedule: Vec<(f32, f32, SegmentType)>,
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

fn boundaries_from_verdicts(times: &[f32], verdicts: &[SegmentType]) -> Vec<f32> {
    let decisions: Vec<(f32, ScoutDecision)> = times
        .iter()
        .zip(verdicts.iter())
        .map(|(&t, &v)| {
            let leaning = if v == SegmentType::Speech { 1.0 } else { 0.0 };
            (t, ScoutDecision { leaning_score: leaning, confidence: 1.0 })
        })
        .collect();
    let segs = smooth_and_segment(&decisions);
    segs.windows(2).map(|w| w[1].start_sec).collect()
}

struct Column {
    correct_a: usize,
    correct_b: usize,
    correct_d: usize,
    total: usize,
    bounds_a: Vec<f32>,
    bounds_b: Vec<f32>,
    cv_ioi: Vec<f32>,
}

/// ΦΡΕΣΚΟΣ ορισμός — αντίγραφο του fixture_battery.rs's run(), ΙΔΙΑ
/// λογική (νέος MfccAnalyzer ανά παράθυρο, μηδέν ιστορικό).
fn run_fresh(mono: &[f32], schedule: &[(f32, f32, SegmentType)]) -> Column {
    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

    let mut scout = SegmentScout::new();
    let mut start = 0usize;

    let mut times = Vec::new();
    let mut real_decisions: Vec<(f32, ScoutDecision)> = Vec::new();
    let mut verdicts_b = Vec::new();
    let mut verdicts_d = Vec::new();
    let mut cv_ioi_seq = Vec::new();
    let mut correct = [0usize; 3];
    let mut total = 0usize;

    while start + win_samples <= mono.len() {
        let end = start + win_samples;
        let mono_slice = &mono[start..end];
        let start_sec = start as f32 / sample_rate as f32;

        let mut mfcc_analyzer = lineos_corpus::mfcc::MfccAnalyzer::new();
        let mut mfccs = Vec::new();
        let mut f = 0;
        while f + 1024 <= mono_slice.len() {
            mfccs.push(mfcc_analyzer.compute(&mono_slice[f..f + 1024]));
            f += 512;
        }
        let cepstral_flux = compute_cepstral_flux(&mfccs);
        let meas: ScoutMeasurements = scout.measure(mono_slice, cepstral_flux, sample_rate);
        let dec_a = compute_scout_decision(&meas);

        let v_a = if dec_a.leaning_score >= 0.5 { SegmentType::Speech } else { SegmentType::Music };
        let v_b = if meas.cepstral_flux >= MID_FLUX { SegmentType::Speech } else { SegmentType::Music };
        let v_d = SegmentType::Speech;

        let center = start_sec + WINDOW_SECS / 2.0;
        let exp = expected_type(schedule, center);
        if v_a == exp { correct[0] += 1; }
        if v_b == exp { correct[1] += 1; }
        if v_d == exp { correct[2] += 1; }
        total += 1;

        times.push(start_sec);
        real_decisions.push((start_sec, dec_a));
        verdicts_b.push(v_b);
        verdicts_d.push(v_d);
        cv_ioi_seq.push(meas.cv_ioi);

        start += hop_samples;
    }

    let bounds_a: Vec<f32> = smooth_and_segment(&real_decisions).windows(2).map(|w| w[1].start_sec).collect();
    let bounds_b = boundaries_from_verdicts(&times, &verdicts_b);

    Column {
        correct_a: correct[0],
        correct_b: correct[1],
        correct_d: correct[2],
        total,
        bounds_a,
        bounds_b,
        cv_ioi: cv_ioi_seq,
    }
}

/// ΚΥΛΙΟΜΕΝΟΣ ορισμός — καλεί ΑΠΕΥΘΕΙΑΣ το ΠΡΑΓΜΑΤΙΚΟ run_trunk_pass.
/// Καμία αναπαραγωγή του MFCC ring: τα cv_ioi_sequence/cepstral_flux_sequence
/// και τα boundaries έρχονται ΑΥΤΟΥΣΙΑ από το TrunkReport.
fn run_rolling(dump_path: &Path, schedule: &[(f32, f32, SegmentType)]) -> Column {
    let report = run_trunk_pass(dump_path, false).unwrap_or_else(|e| panic!("run_trunk_pass: {e}"));
    let cv_ioi_seq = report.metrics.cv_ioi_sequence.clone();
    let flux_seq = report.metrics.cepstral_flux_sequence.clone();
    assert_eq!(cv_ioi_seq.len(), flux_seq.len(), "cv_ioi/flux sequence length mismatch");

    let mut times = Vec::new();
    let mut verdicts_b = Vec::new();
    let mut verdicts_d = Vec::new();
    let mut correct = [0usize; 3];
    let total = cv_ioi_seq.len();

    for i in 0..total {
        let start_sec = i as f32 * HOP_SECS;
        let meas = ScoutMeasurements { cv_ioi: cv_ioi_seq[i], cepstral_flux: flux_seq[i] };
        let dec_a = compute_scout_decision(&meas);

        let v_a = if dec_a.leaning_score >= 0.5 { SegmentType::Speech } else { SegmentType::Music };
        let v_b = if meas.cepstral_flux >= MID_FLUX { SegmentType::Speech } else { SegmentType::Music };
        let v_d = SegmentType::Speech;

        let center = start_sec + WINDOW_SECS / 2.0;
        let exp = expected_type(schedule, center);
        if v_a == exp { correct[0] += 1; }
        if v_b == exp { correct[1] += 1; }
        if v_d == exp { correct[2] += 1; }

        times.push(start_sec);
        verdicts_b.push(v_b);
        verdicts_d.push(v_d);
    }

    let bounds_a: Vec<f32> = report.boundaries.windows(2).map(|w| w[1].start_sec).collect();
    let bounds_b = boundaries_from_verdicts(&times, &verdicts_b);

    Column {
        correct_a: correct[0],
        correct_b: correct[1],
        correct_d: correct[2],
        total,
        bounds_a,
        bounds_b,
        cv_ioi: cv_ioi_seq,
    }
}

fn pct(n: usize, total: usize) -> f32 {
    100.0 * n as f32 / total as f32
}

fn main() {
    use SegmentType::{Music, Speech};
    std::fs::create_dir_all(DUMP_DIR).unwrap();

    let fixtures = vec![
        Fixture { label: "1 ΣΚΕΤΗ ΑΦΗΓΗΣΗ", wav: "test1_narration_only.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "2 ΣΚΕΤΟ BED", wav: "test2_bed_only.wav", schedule: vec![(0.0, 120.0, Music)] },
        Fixture { label: "3 ΑΦΗΓΗΣΗ+BED -20dB", wav: "test3_narration_bed_m20dB.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "4 ΑΦΗΓΗΣΗ+BED -12dB", wav: "test4_narration_bed_m12dB.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture {
            label: "5 ΕΝΑΛΛΑΓΗ 30/60/90",
            wav: "test5_alternation_30_60_90.wav",
            schedule: vec![(0.0, 30.0, Speech), (30.0, 60.0, Music), (60.0, 90.0, Speech), (90.0, 120.0, Music)],
        },
        Fixture { label: "6 BED ΜΠΑΙΝΕΙ ΣΤΑ 60s", wav: "test6_bed_enters_60s.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture {
            label: "7 ΜΗ-ΣΤΡΟΓΓΥΛΕΣ 17/41/68/94",
            wav: "test7_nonround_17_41_68_94.wav",
            schedule: vec![(0.0, 17.0, Speech), (17.0, 41.0, Music), (41.0, 68.0, Speech), (68.0, 94.0, Music), (94.0, 120.0, Speech)],
        },
        Fixture {
            label: "8 ΣΥΝΤΟΜΕΣ 20s x6",
            wav: "test8_short_20s_x6.wav",
            schedule: vec![
                (0.0, 20.0, Speech), (20.0, 40.0, Music), (40.0, 60.0, Speech),
                (60.0, 80.0, Music), (80.0, 100.0, Speech), (100.0, 120.0, Music),
            ],
        },
        Fixture {
            label: "9 ΑΣΥΜΜΕΤΡΕΣ",
            wav: "test9_asymmetric.wav",
            schedule: vec![(0.0, 45.0, Speech), (45.0, 53.0, Music), (53.0, 103.0, Speech), (103.0, 111.0, Music), (111.0, 120.0, Speech)],
        },
        Fixture {
            label: "10 BED ΣΥΝΕΧΙΖΕΙ ΑΠΟ ΚΑΤΩ",
            wav: "test10_bed_continues_under.wav",
            schedule: vec![(0.0, 40.0, Speech), (40.0, 60.0, Music), (60.0, 120.0, Speech)],
        },
        Fixture {
            label: "11 ΑΛΛΟ BED, ΙΔΙΟ ΣΧΗΜΑ ΜΕ (7)",
            wav: "test11_altbed_17_41_68_94.wav",
            schedule: vec![(0.0, 17.0, Speech), (17.0, 41.0, Music), (41.0, 68.0, Speech), (68.0, 94.0, Music), (94.0, 120.0, Speech)],
        },
        Fixture {
            label: "12 ΑΛΛΟΣ ΑΦΗΓΗΤΗΣ, ΙΔΙΟ ΣΧΗΜΑ ΜΕ (7)",
            wav: "test12_altnarrator_17_41_68_94.wav",
            schedule: vec![(0.0, 17.0, Speech), (17.0, 41.0, Music), (41.0, 68.0, Speech), (68.0, 94.0, Music), (94.0, 120.0, Speech)],
        },
    ];

    println!("ΒΗΜΑ 0: run_trunk_pass καλείται ΑΠΕΥΘΕΙΑΣ (καμία αναπαραγωγή λογικής).");
    println!("Μετατροπή μορφής wav->raw dump (headerless f32 LE interleaved), ίδια /32768.0 σύμβαση με fixture_battery.rs.\n");
    println!("MID_CV={MID_CV:.4} MID_FLUX={MID_FLUX:.4} ΑΝΟΧΗ_ΟΡΙΩΝ={BOUNDARY_TOLERANCE_SECS}s\n");

    let mut summary: Vec<(String, f32, f32)> = Vec::new();

    for fx in &fixtures {
        let wav_path = format!("{WORK}/{}", fx.wav);
        let dump_path = Path::new(DUMP_DIR).join(format!("{}.f32dump", fx.wav));
        wav_to_raw_dump(&wav_path, &dump_path);

        let (left, right) = read_wav_stereo_f32(&wav_path);
        let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();

        let fresh = run_fresh(&mono, &fx.schedule);
        let rolling = run_rolling(&dump_path, &fx.schedule);

        let count_match = fresh.total == rolling.total;
        let max_cv_diff = if count_match {
            fresh.cv_ioi.iter().zip(rolling.cv_ioi.iter())
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max)
        } else {
            f32::NAN
        };

        println!("=== {} ({}) ===", fx.label, fx.wav);
        println!(
            "  ΠΙΣΤΟΤΗΤΑ: πλήθος παραθύρων φρέσκο={} κυλιόμενο={} {} · max|Δcv_ioi|={:.6}",
            fresh.total, rolling.total,
            if count_match { "ΣΥΜΦΩΝΙΑ" } else { "⚠ ΑΣΥΜΦΩΝΙΑ" },
            max_cv_diff
        );
        println!(
            "  ΦΡΕΣΚΟΣ    (n={}): Α={:>3}/{} ({:.1}%)  Β={:>3}/{} ({:.1}%)  Δ={:>3}/{} ({:.1}%)",
            fresh.total, fresh.correct_a, fresh.total, pct(fresh.correct_a, fresh.total),
            fresh.correct_b, fresh.total, pct(fresh.correct_b, fresh.total),
            fresh.correct_d, fresh.total, pct(fresh.correct_d, fresh.total),
        );
        println!(
            "  ΚΥΛΙΟΜΕΝΟΣ (n={}): Α={:>3}/{} ({:.1}%)  Β={:>3}/{} ({:.1}%)  Δ={:>3}/{} ({:.1}%)",
            rolling.total, rolling.correct_a, rolling.total, pct(rolling.correct_a, rolling.total),
            rolling.correct_b, rolling.total, pct(rolling.correct_b, rolling.total),
            rolling.correct_d, rolling.total, pct(rolling.correct_d, rolling.total),
        );

        let exp_bounds = expected_boundaries(&fx.schedule);
        println!("  αναμενόμενα όρια: {:?}", exp_bounds);
        for (name, col) in [("ΦΡΕΣΚΟΣ", &fresh), ("ΚΥΛΙΟΜΕΝΟΣ", &rolling)] {
            for (rule, prod) in [("Α", &col.bounds_a), ("Β", &col.bounds_b)] {
                let (found, missed, extra) = match_boundaries(&exp_bounds, prod, BOUNDARY_TOLERANCE_SECS);
                println!(
                    "    {name} όρια {rule}: παρήχθησαν={:?}  βρέθηκαν={found}/{} χάθηκαν={missed} περίσσεψαν={extra}",
                    prod, exp_bounds.len()
                );
            }
        }
        println!();

        summary.push((fx.label.to_string(), pct(fresh.correct_a, fresh.total), pct(rolling.correct_a, rolling.total)));
    }

    println!("── ΠΙΝΑΚΑΣ 12×2 — ορθότητα κανόνα Α (σημερινή προβολή), ΦΡΕΣΚΟΣ έναντι ΚΥΛΙΟΜΕΝΟΥ ──");
    for (label, f, r) in &summary {
        println!("  {:<32} ΦΡΕΣΚΟΣ={:>5.1}%  ΚΥΛΙΟΜΕΝΟΣ={:>5.1}%  Δ={:>+6.1}pp", label, f, r, r - f);
    }
}
