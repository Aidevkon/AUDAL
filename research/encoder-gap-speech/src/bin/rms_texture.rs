//! MEASURE 2026-09-16: ξεχωρίζει η διακύμανση RMS εκεί που οι δύο άξονες
//! του Scout (ρυθμός επιθέσεων/cv_ioi, αλλαγή χροιάς/cepstral_flux)
//! απέτυχαν; Read-only, ΜΗΔΕΝ αλλαγή στον scout/centroids/κατώφλια.
//!
//! ΤΟ ΟΡΓΑΝΟ: qw_env (trunk_pass.rs:581,762-781) — γραμμική RMS ανά
//! 100ms πάνω στο mono downmix (l+r)*0.5 (trunk_pass.rs:695, ΙΔΙΟΣ
//! τύπος με scan_file). ΔΕΝ εκθέτεται από το TrunkReport (μόνο
//! boundaries/metrics/acx_interior_noise_floor/quiet_window_split_dbfs/
//! mains_line/input_fundamental — καμία qw_env) — ΔΕΝ είναι προσβάσιμο
//! από το εργαλείο χωρίς να ξαναϋπολογιστεί. Αντίγραφο εδώ, ΙΔΙΟΣ
//! τύπος: `e = mean(s²) πάνω σε 4800 δείγματα (100ms@48kHz), push
//! sqrt(e)` — γραμμή-προς-γραμμή ίδιο με trunk_pass.rs:766-777, μόνο
//! που η dB μετατροπή γίνεται εδώ (η παραγωγή κρατάει qw_env γραμμικό
//! επίτηδες, για το longest_run_below/above — δική μας ανάγκη, dB, όχι
//! δική της).
//!
//! ΣΙΩΠΗ: αν κάποιο από τα 4800 δείγματα δώσει e<=0 (πρακτικά δεν
//! συμβαίνει σε πραγματικό ήχο με dither/θόρυβο, αλλά δηλωμένο):
//! δάπεδο στα -120.0 dBFS πριν το log, ώστε το log(0) να μην σκάει τον
//! υπολογισμό τυπικής απόκλισης/εκατοστημορίων.
//!
//! Ξαναχρησιμοποιεί τα έξι WAV του προηγούμενου MEASURE
//! (/tmp/scout-groundtruth/test*.wav) — ΔΕΝ παράγει νέο ήχο.
//!
//! ΧΡΗΣΗ: cargo run --release --bin rms_texture

use lineos_corpus::scout::SegmentType;
use sp314_dsp::analysis::scout::SegmentScout;

const WORK: &str = "/tmp/scout-groundtruth";
const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const QW_WINDOW: usize = 4_800; // 100ms @ 48kHz, ΑΥΤΟΥΣΙΟ από trunk_pass.rs:204
const SILENCE_FLOOR_DBFS: f64 = -120.0;

fn read_wav_stereo_f32(path: &str) -> (Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let samples: Vec<f32> = reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
    let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
    (left, right)
}

/// ΑΥΤΟΥΣΙΟ από trunk_pass.rs:766-777 (γραμμικό RMS ανά 100ms),
/// + μετατροπή σε dB εδώ (20*log10(v), δάπεδο -120dBFS σε v<=0).
fn qw_env_db(mono: &[f32]) -> Vec<f64> {
    let mut out = Vec::with_capacity(mono.len() / QW_WINDOW);
    let mut sum_sq = 0.0f32;
    let mut count = 0usize;
    for &s in mono {
        sum_sq += s * s;
        count += 1;
        if count == QW_WINDOW {
            let e = (sum_sq / QW_WINDOW as f32) as f64; // mean square, ΙΔΙΟ με trunk_pass.rs:769
            let rms_linear = e.max(0.0).sqrt(); // ΙΔΙΟ με trunk_pass.rs:777
            let db = if rms_linear > 0.0 { 20.0 * rms_linear.log10() } else { SILENCE_FLOOR_DBFS };
            out.push(db.max(SILENCE_FLOOR_DBFS));
            sum_sq = 0.0;
            count = 0;
        }
    }
    out
}

fn std_dev(xs: &[f64]) -> f64 {
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    var.sqrt()
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn median(xs: &mut Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = xs.len();
    if n % 2 == 0 {
        (xs[n / 2 - 1] + xs[n / 2]) / 2.0
    } else {
        xs[n / 2]
    }
}

fn main() {
    let cases = [
        ("1 ΣΚΕΤΗ ΑΦΗΓΗΣΗ", "test1_narration_only.wav", "AllSpeech"),
        ("2 ΣΚΕΤΟ BED", "test2_bed_only.wav", "AllMusic"),
        ("3 ΑΦΗΓΗΣΗ+BED -20dB", "test3_narration_bed_m20dB.wav", "AllSpeech"),
        ("4 ΑΦΗΓΗΣΗ+BED -12dB", "test4_narration_bed_m12dB.wav", "AllSpeech"),
        ("5 ΕΝΑΛΛΑΓΗ 30/60/90", "test5_alternation_30_60_90.wav", "Alternating"),
        ("6 BED ΜΠΑΙΝΕΙ ΣΤΑ 60s", "test6_bed_enters_60s.wav", "AllSpeech"),
    ];

    let expected_speech = |gt: &str, center: f32| -> bool {
        match gt {
            "AllMusic" => false,
            "Alternating" => {
                let block = (center / 30.0).floor() as i64;
                block % 2 == 0
            }
            _ => true,
        }
    };

    println!("{:<24} {:>8} {:>10} {:>10} {:>10} {:>10}", "δοκίμιο", "παράθυρα", "σ(dB)διαμ", "σ(dB)ευρος", "p90-p10 διαμ", "p90-p10 ευρος");

    for (label, wav, gt) in cases {
        let path = format!("{WORK}/{wav}");
        let (left, right) = read_wav_stereo_f32(&path);
        let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
        let env_db = qw_env_db(&mono);

        let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE as usize;
        let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
        let hop_samples = (HOP_SECS * sample_rate as f32) as usize;
        let samples_per_analysis_window = win_samples / QW_WINDOW; // 240000/4800 = 50

        // ΙΔΙΑ σάρωση παραθύρων με scan_file, ώστε τα timestamps να ταιριάζουν
        let mut scout = SegmentScout::new();
        let mut start = 0usize;

        let mut all_std: Vec<f64> = Vec::new();
        let mut all_p9010: Vec<f64> = Vec::new();
        let mut correct_std: Vec<f64> = Vec::new();
        let mut wrong_std: Vec<f64> = Vec::new();
        let mut correct_p9010: Vec<f64> = Vec::new();
        let mut wrong_p9010: Vec<f64> = Vec::new();
        let mut rows: Vec<(f32, f64, f64, bool)> = Vec::new();

        while start + win_samples <= mono.len() {
            let end = start + win_samples;
            let mono_slice = &mono[start..end];
            let start_sec = start as f32 / sample_rate as f32;

            let qw_start = start / QW_WINDOW;
            let qw_slice = &env_db[qw_start..(qw_start + samples_per_analysis_window).min(env_db.len())];

            let sd = std_dev(qw_slice);
            let mut sorted = qw_slice.to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let spread = percentile(&sorted, 0.90) - percentile(&sorted, 0.10);

            // scout verdict, ίδιο μονοπάτι με scout_groundtruth.rs
            let mut mfcc_analyzer = lineos_corpus::mfcc::MfccAnalyzer::new();
            let mut mfccs = Vec::new();
            let mut f = 0;
            while f + 1024 <= mono_slice.len() {
                mfccs.push(mfcc_analyzer.compute(&mono_slice[f..f + 1024]));
                f += 512;
            }
            let cepstral_flux = lineos_corpus::scout::compute_cepstral_flux(&mfccs);
            let meas = scout.measure(mono_slice, cepstral_flux, sample_rate as u32);
            let dec = lineos_corpus::scout::compute_scout_decision(&meas);
            let raw_verdict_speech = dec.leaning_score >= 0.5;
            let center = start_sec + WINDOW_SECS / 2.0;
            let expected = expected_speech(gt, center);
            let is_correct = raw_verdict_speech == expected;
            let _ = SegmentType::Speech; // silence unused-import guard

            all_std.push(sd);
            all_p9010.push(spread);
            if is_correct {
                correct_std.push(sd);
                correct_p9010.push(spread);
            } else {
                wrong_std.push(sd);
                wrong_p9010.push(spread);
            }
            rows.push((start_sec, sd, spread, is_correct));

            start += hop_samples;
        }

        let mut all_std_m = all_std.clone();
        let mut all_p9010_m = all_p9010.clone();
        let std_med = median(&mut all_std_m);
        let std_min = all_std.iter().cloned().fold(f64::INFINITY, f64::min);
        let std_max = all_std.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let p9010_med = median(&mut all_p9010_m);
        let p9010_min = all_p9010.iter().cloned().fold(f64::INFINITY, f64::min);
        let p9010_max = all_p9010.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        println!(
            "{label:<24} {:>8} {:>10.2} {:>10} {:>10.2} {:>10}",
            rows.len(),
            std_med,
            format!("{std_min:.2}-{std_max:.2}"),
            p9010_med,
            format!("{p9010_min:.2}-{p9010_max:.2}")
        );

        // λεπτομέρεια: σωστά vs λάθη
        if !correct_std.is_empty() {
            let mut c = correct_std.clone();
            let mut cp = correct_p9010.clone();
            println!(
                "  σωστά (n={}): σ διάμεσος={:.2} p90-p10 διάμεσος={:.2}",
                correct_std.len(), median(&mut c), median(&mut cp)
            );
        }
        if !wrong_std.is_empty() {
            let mut w = wrong_std.clone();
            let mut wp = wrong_p9010.clone();
            println!(
                "  λάθη (n={}): σ διάμεσος={:.2} p90-p10 διάμεσος={:.2}",
                wrong_std.len(), median(&mut w), median(&mut wp)
            );
        } else {
            println!("  λάθη: κανένα");
        }

        println!("  --- ανά παράθυρο (t, σ dB, p90-p10 dB, scout σωστά;) ---");
        for (t, sd, spread, ok) in &rows {
            println!("  {t:>7.2}s  σ={sd:>7.3}  p90-p10={spread:>7.3}  {}", if *ok { "ναι" } else { "ΟΧΙ" });
        }
        println!();
    }
}
