//! MEASURE 2026-09-16: η επιπεδότητα (spectral flatness) στο δοκίμιο
//! 10. Read-only, ΜΗΔΕΝ αλλαγή στην παραγωγή, ΜΗΔΕΝ αλλαγή στον
//! scout/centroids/κατώφλια/ανιχνευτή. ΜΗΔΕΝ αναδημιουργία DSP: το
//! εργαλείο καλεί το ΠΡΑΓΜΑΤΙΚΟ sp314_dsp::analysis::spectral::
//! spectral_flatness ως μαύρο κουτί, επαναλαμβανόμενα, πάνω σε
//! υπο-τμήματα.
//!
//! ΒΗΜΑ 0.1, επαληθευμένο στον κώδικα (spectral.rs:78-115):
//!   pub fn spectral_flatness(signal: &[f32]) -> f32
//! Δέχεται ΩΜΟ mono buffer δειγμάτων (ΟΧΙ φάσμα, ΟΧΙ magnitudes —
//! κάνει το ΔΙΚΟ της StftEngine::forward εσωτερικά). Επιστρέφει έναν
//! ΜΟΝΟ αριθμό στο [0,1] — ήδη μέσος όρος geometric/arithmetic mean
//! ratio σε ΟΛΑ τα STFT πλαίσια του σήματος που δόθηκε (0=τονικό,
//! 1=επίπεδο/θορυβώδες). Το StoredQuality.spectral_flatness στο cert
//! (μία από τις εννιά καρφωμένες σταθερές, 0.12) είναι ΑΛΛΟ πράγμα —
//! ΔΕΝ αγγίζεται, δεν αφορά αυτή τη μέτρηση.
//!
//! Επειδή η συνάρτηση επιστρέφει ΗΔΗ μέσο όρο σε όλο το σήμα που
//! δέχεται, για διάμεσο/διακύμανση ΜΕΣΑ σε ένα παράθυρο 5s καλείται
//! ΕΠΑΝΕΙΛΗΜΜΕΝΑ σε 10 υπο-τμήματα των 0.5s (24 000 δείγματα
//! @48kHz) — μαύρο κουτί, καμία επαφή με τα εσωτερικά της.
//!
//! ΔΥΟ ΕΚΔΟΧΕΣ ΜΕΤΡΟΥ ανά παράθυρο 5s, πάνω στις 10 τιμές sub-chunk:
//!   Α. διάμεσος επιπεδότητας
//!   Β. τυπική απόκλιση επιπεδότητας (η «διαμόρφωση» μέσα στο παράθυρο)
//!
//! Δοκίμια: τα δώδεκα του F-110/F-111 (/tmp/scout-groundtruth/
//! test*.wav) — ΜΗΔΕΝ νέο υλικό.
//!
//! ΧΡΗΣΗ: cargo run --release --bin flatness_probe

use lineos_corpus::scout::SegmentType;
use sp314_dsp::analysis::spectral::spectral_flatness;

const WORK: &str = "/tmp/scout-groundtruth";
const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const SUBCHUNK_SECS: f32 = 0.5;

fn read_wav_stereo_f32(path: &str) -> (Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let samples: Vec<f32> = reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
    let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
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

fn std_dev(xs: &[f32]) -> f32 {
    let n = xs.len() as f32;
    if n == 0.0 {
        return f32::NAN;
    }
    let mean = xs.iter().sum::<f32>() / n;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
    var.sqrt()
}

fn range(xs: &[f32]) -> (f32, f32) {
    (xs.iter().cloned().fold(f32::INFINITY, f32::min), xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max))
}

/// Ανά παράθυρο 5s: (median_flatness_ανά_subchunk, std_flatness_ανά_subchunk).
fn window_flatness(mono_slice: &[f32], sample_rate: u32) -> (f32, f32) {
    let sub_samples = (SUBCHUNK_SECS * sample_rate as f32) as usize;
    let mut vals = Vec::new();
    let mut pos = 0;
    while pos + sub_samples <= mono_slice.len() {
        vals.push(spectral_flatness(&mono_slice[pos..pos + sub_samples]));
        pos += sub_samples;
    }
    (median(&vals), std_dev(&vals))
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

    println!("ΥΠΟ-ΤΜΗΜΑ={SUBCHUNK_SECS}s (10 ανά παράθυρο 5s) — spectral_flatness ως μαύρο κουτί\n");

    // ΤΟ ΚΡΙΣΙΜΟ ΠΡΩΤΟ: δοκίμιο 10, δύο σύνολα.
    {
        let fx = fixtures.iter().find(|f| f.wav.starts_with("test10")).unwrap();
        let path = format!("{WORK}/{}", fx.wav);
        let (left, right) = read_wav_stereo_f32(&path);
        let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
        let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
        let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
        let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

        let mut with_voice_med = Vec::new();
        let mut with_voice_std = Vec::new();
        let mut without_voice_med = Vec::new();
        let mut without_voice_std = Vec::new();

        let mut start = 0usize;
        while start + win_samples <= mono.len() {
            let end = start + win_samples;
            let mono_slice = &mono[start..end];
            let start_sec = start as f32 / sample_rate as f32;
            let center = start_sec + WINDOW_SECS / 2.0;
            let (med, std) = window_flatness(mono_slice, sample_rate);
            match expected_type(&fx.schedule, center) {
                Speech => {
                    with_voice_med.push(med);
                    with_voice_std.push(std);
                }
                Music => {
                    without_voice_med.push(med);
                    without_voice_std.push(std);
                }
            }
            start += hop_samples;
        }

        println!("=== ΤΟ ΚΡΙΣΙΜΟ: δοκίμιο 10, ίδιο bed, μία μεταβλητή (φωνή ή όχι) ===");
        let (wv_lo, wv_hi) = range(&with_voice_med);
        let (nv_lo, nv_hi) = range(&without_voice_med);
        println!(
            "  ΜΕ φωνή (0-40s,60-120s, n={}):    διάμεσος(Α)={:.4}  εύρος={:.4}-{:.4}",
            with_voice_med.len(), median(&with_voice_med), wv_lo, wv_hi
        );
        println!(
            "  ΧΩΡΙΣ φωνή (40-60s, n={}):        διάμεσος(Α)={:.4}  εύρος={:.4}-{:.4}",
            without_voice_med.len(), median(&without_voice_med), nv_lo, nv_hi
        );
        // Συμμετρικός τύπος, ανεξάρτητος από ποιο σύνολο είναι ψηλότερα:
        // gap = max(τα δύο ελάχιστα) − min(τα δύο μέγιστα). >0 ⇒ κενό, <=0 ⇒ επικάλυψη.
        let gap = wv_lo.max(nv_lo) - wv_hi.min(nv_hi);
        println!("  ΚΕΝΟ/ΕΠΙΚΑΛΥΨΗ (Α): {:.4} ({})", gap, if gap > 0.0 { "ΚΕΝΟ" } else { "ΕΠΙΚΑΛΥΨΗ" });

        let (wv_std_lo, wv_std_hi) = range(&with_voice_std);
        let (nv_std_lo, nv_std_hi) = range(&without_voice_std);
        println!(
            "  ΜΕ φωνή, διακύμανση(Β):           διάμεσος={:.4}  εύρος={:.4}-{:.4}",
            median(&with_voice_std), wv_std_lo, wv_std_hi
        );
        println!(
            "  ΧΩΡΙΣ φωνή, διακύμανση(Β):        διάμεσος={:.4}  εύρος={:.4}-{:.4}",
            median(&without_voice_std), nv_std_lo, nv_std_hi
        );
        let gap_b = wv_std_lo.max(nv_std_lo) - wv_std_hi.min(nv_std_hi);
        println!("  ΚΕΝΟ/ΕΠΙΚΑΛΥΨΗ (Β): {:.4} ({})\n", gap_b, if gap_b > 0.0 { "ΚΕΝΟ" } else { "ΕΠΙΚΑΛΥΨΗ" });
    }

    // Φρουρός: (1) έναντι (2)
    {
        let mut medians_by_fixture: std::collections::HashMap<&str, (f32, f32, f32)> = std::collections::HashMap::new();
        for fx in fixtures.iter().filter(|f| f.wav.starts_with("test1_") || f.wav.starts_with("test2_")) {
            let path = format!("{WORK}/{}", fx.wav);
            let (left, right) = read_wav_stereo_f32(&path);
            let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
            let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
            let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
            let hop_samples = (HOP_SECS * sample_rate as f32) as usize;
            let mut meds = Vec::new();
            let mut start = 0usize;
            while start + win_samples <= mono.len() {
                let (med, _std) = window_flatness(&mono[start..start + win_samples], sample_rate);
                meds.push(med);
                start += hop_samples;
            }
            let (lo, hi) = range(&meds);
            medians_by_fixture.insert(fx.label, (median(&meds), lo, hi));
        }
        println!("=== Ο ΦΡΟΥΡΟΣ: (1) έναντι (2) ===");
        for label in ["1 ΣΚΕΤΗ ΑΦΗΓΗΣΗ", "2 ΣΚΕΤΟ BED"] {
            if let Some(&(med, lo, hi)) = medians_by_fixture.get(label) {
                println!("  {label:<24} διάμεσος={med:.4}  εύρος={lo:.4}-{hi:.4}");
            }
        }
        println!();
    }

    // Πλήρης πίνακας, όλα τα δώδεκα
    println!("=== ΠΛΗΡΗΣ ΠΙΝΑΚΑΣ (12 δοκίμια) ===");
    for fx in &fixtures {
        let path = format!("{WORK}/{}", fx.wav);
        let (left, right) = read_wav_stereo_f32(&path);
        let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
        let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
        let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
        let hop_samples = (HOP_SECS * sample_rate as f32) as usize;
        let mut meds = Vec::new();
        let mut stds = Vec::new();
        let mut start = 0usize;
        while start + win_samples <= mono.len() {
            let (med, std) = window_flatness(&mono[start..start + win_samples], sample_rate);
            meds.push(med);
            stds.push(std);
            start += hop_samples;
        }
        let (mlo, mhi) = range(&meds);
        let (slo, shi) = range(&stds);
        println!(
            "{:<24} Α(διάμ/εύρος)={:.4} [{:.4}-{:.4}]   Β(διάμ/εύρος)={:.4} [{:.4}-{:.4}]",
            fx.label, median(&meds), mlo, mhi, median(&stds), slo, shi
        );
    }
}
