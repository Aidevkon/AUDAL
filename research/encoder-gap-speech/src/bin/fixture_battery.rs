//! MEASURE 2026-09-16: το δοκίμιο μεγαλώνει εκεί που κρίνεται. Read-only,
//! ΜΗΔΕΝ αλλαγή στον scout/centroids/κατώφλια. Δώδεκα δοκίμια (έξι παλιά
//! + έξι νέα, όλα με πραγματικές εναλλαγές), τέσσερις κανόνες απόφασης
//! (Α=σημερινή προβολή, Β=μόνο flux, Γ=μόνο cv_ioi, Δ=εκφυλισμένος
//! «πάντα ομιλία»), ορθότητα ΚΑΙ όρια ανά κανόνα.
//!
//! ΙΔΙΕΣ συναρτήσεις παραγωγής: sp314_dsp::analysis::scout_scanner::
//! scan_file (WINDOW_SECS/HOP_SECS=5.0/1.0) + lineos_corpus::scout::
//! smooth_and_segment (ο τεμαχιστής). Για τα όρια των κανόνων Β/Γ/Δ,
//! συνθέτουμε ScoutDecision{leaning_score: 1.0/0.0, confidence: 1.0}
//! ανά παράθυρο (βάσει της ΩΜΗΣ ετυμηγορίας του κανόνα) και περνάμε
//! στο ΙΔΙΟ smooth_and_segment — έτσι η EMA εξομάλυνση εφαρμόζεται
//! ομοιόμορφα και στους τέσσερις κανόνες, ίδιος μηχανισμός με την
//! παραγωγή.
//!
//! ΑΝΟΧΗ ΟΡΙΩΝ, δηλωμένη πριν το τρέξιμο: ±5.0s (πλάτος ενός
//! παραθύρου ανάλυσης — φυσικό όριο ανάλυσης του οργάνου, όχι
//! συντονισμένο στο αποτέλεσμα).
//!
//! ΧΡΗΣΗ: cargo run --release --bin fixture_battery

use lineos_corpus::scout::{
    compute_cepstral_flux, compute_scout_decision, smooth_and_segment, ScoutDecision, ScoutMeasurements,
    SegmentType,
};
use sp314_dsp::analysis::scout::SegmentScout;

const WORK: &str = "/tmp/scout-groundtruth";
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

/// Χρονοδιάγραμμα τύπου, σε δευτερόλεπτα: (start, end, type). Χρησιμοποιείται
/// ΚΑΙ για το per-window ground truth (τύπος στο κέντρο του παραθύρου) ΚΑΙ
/// για τα αναμενόμενα όρια (τα σημεία αλλαγής τύπου).
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

fn run(fx: &Fixture) {
    let path = format!("{WORK}/{}", fx.wav);
    let (left, right) = read_wav_stereo_f32(&path);
    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();

    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

    let mut scout = SegmentScout::new();
    let mut start = 0usize;

    let mut times = Vec::new();
    let mut real_decisions: Vec<(f32, ScoutDecision)> = Vec::new();
    let mut verdicts_a = Vec::new();
    let mut verdicts_b = Vec::new();
    let mut verdicts_g = Vec::new();
    let mut verdicts_d = Vec::new();
    let mut correct = [0usize; 4];
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
        let v_g = if !meas.cv_ioi.is_nan() && meas.cv_ioi >= MID_CV { SegmentType::Speech } else { SegmentType::Music };
        let v_d = SegmentType::Speech; // εκφυλισμένος: πάντα ομιλία

        let center = start_sec + WINDOW_SECS / 2.0;
        let exp = expected_type(&fx.schedule, center);
        if v_a == exp { correct[0] += 1; }
        if v_b == exp { correct[1] += 1; }
        if v_g == exp { correct[2] += 1; }
        if v_d == exp { correct[3] += 1; }
        total += 1;

        times.push(start_sec);
        real_decisions.push((start_sec, dec_a));
        verdicts_a.push(v_a);
        verdicts_b.push(v_b);
        verdicts_g.push(v_g);
        verdicts_d.push(v_d);

        start += hop_samples;
    }

    let exp_bounds = expected_boundaries(&fx.schedule);

    let prod_a: Vec<f32> = smooth_and_segment(&real_decisions).windows(2).map(|w| w[1].start_sec).collect();
    let prod_b = boundaries_from_verdicts(&times, &verdicts_b);
    let prod_g = boundaries_from_verdicts(&times, &verdicts_g);
    let prod_d = boundaries_from_verdicts(&times, &verdicts_d);

    println!("=== {} ({}) ===", fx.label, fx.wav);
    println!(
        "  ορθότητα (n={total}): Α={:>3}/{total} ({:.1}%)  Β={:>3}/{total} ({:.1}%)  Γ={:>3}/{total} ({:.1}%)  Δ={:>3}/{total} ({:.1}%)",
        correct[0], 100.0*correct[0] as f32/total as f32,
        correct[1], 100.0*correct[1] as f32/total as f32,
        correct[2], 100.0*correct[2] as f32/total as f32,
        correct[3], 100.0*correct[3] as f32/total as f32,
    );
    println!("  αναμενόμενα όρια: {:?}", exp_bounds);
    for (name, prod) in [("Α", &prod_a), ("Β", &prod_b), ("Γ", &prod_g), ("Δ", &prod_d)] {
        let (found, missed, extra) = match_boundaries(&exp_bounds, prod, BOUNDARY_TOLERANCE_SECS);
        println!(
            "  όρια {name}: παρήχθησαν={:?}  βρέθηκαν={found}/{} χάθηκαν={missed} περίσσεψαν={extra}",
            prod, exp_bounds.len()
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

    println!("MID_CV={MID_CV:.4} MID_FLUX={MID_FLUX:.4} ΑΝΟΧΗ_ΟΡΙΩΝ={BOUNDARY_TOLERANCE_SECS}s\n");
    for fx in &fixtures {
        run(fx);
    }
}
