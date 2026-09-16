//! MEASURE 2026-09-16: ο ταξινομητής Scout σε δοκίμια με γνωστή
//! απάντηση. Καλεί το ΙΔΙΟ όργανο παραγωγής ΑΠΕΥΘΕΙΑΣ, χωρίς πλήρες
//! render και χωρίς pass0_decode_to_dump/run_trunk_pass (δεν
//! χρειάζονται — καμία μέτρηση LUFS/crest/LRA εδώ):
//!   sp314_dsp::analysis::scout_scanner::scan_file  (ίδιο WINDOW_SECS/
//!     HOP_SECS = 5.0/1.0, scout_scanner.rs:6-7, mirror στο
//!     trunk_pass.rs:187-189 που τρέχει η παραγωγή)
//!   lineos_corpus::scout::smooth_and_segment          (ο τεμαχιστής)
//! ΜΗΔΕΝ αντίγραφο λογικής — ΙΔΙΕΣ συναρτήσεις με την παραγωγή.
//!
//! VAD: ΔΕΝ μετράται εδώ. Το πραγματικό production VAD posterior
//! (two_pass.rs:1790-1793, `phi1_p_value.unwrap_or(d.posterior)` όταν
//! `USE_NEURAL_VAD`) ζει ΜΕΣΑ στο NMF two-pass βρόχο, ενεργό μόνο πίσω
//! από `phi1_active && vad_observer.is_some()` (two_pass.rs:1666-1667)
//! — δεν υπάρχει standalone είσοδος χωρίς να στηθεί το NMF two-pass
//! harness. Το VadClassifier+FixedPriors (vad_model.rs) ΕΙΝΑΙ cheap/
//! standalone, αλλά ΔΕΝ είναι αυτό που τρέχει η παραγωγή όταν
//! USE_NEURAL_VAD=true — θα μετρούσε διαφορετικό όργανο. Άρα: μόνο
//! Scout εδώ, όπως επιτρέπει ρητά το task.
//!
//! ΧΡΗΣΗ: cargo run --release --bin scout_groundtruth

use lineos_corpus::scout::{smooth_and_segment, SegmentType};
use sp314_dsp::analysis::scout_scanner::scan_file;

const WINDOW_SECS: f32 = 5.0;
const WORK: &str = "/tmp/scout-groundtruth";

#[derive(Clone, Copy)]
enum GT {
    AllSpeech,
    AllMusic,
    Alternating, // 0-30 Speech, 30-60 Music, 60-90 Speech, 90-120 Music
}

fn expected_type(gt: GT, window_center_sec: f32) -> SegmentType {
    match gt {
        GT::AllSpeech => SegmentType::Speech,
        GT::AllMusic => SegmentType::Music,
        GT::Alternating => {
            let block = (window_center_sec / 30.0).floor() as i64;
            if block % 2 == 0 {
                SegmentType::Speech
            } else {
                SegmentType::Music
            }
        }
    }
}

fn read_wav_stereo_f32(path: &str) -> (Vec<f32>, Vec<f32>, u32) {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let spec = reader.spec();
    assert_eq!(spec.channels, 2, "{path}: expected stereo");
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect();
    let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
    (left, right, spec.sample_rate)
}

fn run_case(label: &str, wav_name: &str, gt: GT, expected_boundaries: usize) {
    let path = format!("{WORK}/{wav_name}");
    let (left, right, sample_rate) = read_wav_stereo_f32(&path);
    let decisions = scan_file(&left, &right, sample_rate);
    let boundaries = smooth_and_segment(&decisions);

    let mut correct = 0usize;
    let mut wrong = 0usize;
    let mut wrong_conf_sum = 0.0f32;

    println!("=== {label} ({wav_name}) ===");
    println!("{:>7}  {:>8}  {:>8}  {:>8}  {:>6}", "t(s)", "leaning", "conf", "verdict", "ok?");
    for (t, dec) in &decisions {
        let raw_verdict = if dec.leaning_score >= 0.5 { SegmentType::Speech } else { SegmentType::Music };
        let center = t + WINDOW_SECS / 2.0;
        let expected = expected_type(gt, center);
        let ok = raw_verdict == expected;
        if ok {
            correct += 1;
        } else {
            wrong += 1;
            wrong_conf_sum += dec.confidence;
        }
        println!(
            "{:>7.2}  {:>8.4}  {:>8.4}  {:>8}  {:>6}",
            t,
            dec.leaning_score,
            dec.confidence,
            format!("{:?}", raw_verdict),
            if ok { "ναι" } else { "ΟΧΙ" }
        );
    }
    let total = correct + wrong;
    let pct_correct = 100.0 * correct as f32 / total.max(1) as f32;
    let avg_wrong_conf = if wrong > 0 { wrong_conf_sum / wrong as f32 } else { f32::NAN };

    println!("\nΤΜΗΜΑΤΑ ΠΟΥ ΠΑΡΗΓΑΓΕ Ο ΤΜΗΜΑΤΟΠΟΙΗΤΗΣ ({}):", boundaries.len());
    for (i, b) in boundaries.iter().enumerate() {
        println!(
            "  {i:>2}  {:>7.2}s - {:>7.2}s  ({:>5.2}s)  {:?}  leaning={:.4} conf={:.4}",
            b.start_sec, b.end_sec, b.end_sec - b.start_sec, b.segment_type, b.avg_leaning, b.avg_confidence
        );
    }
    let actual_boundaries = boundaries.len().saturating_sub(1);
    println!(
        "\nΣΥΝΟΨΗ {label}: παράθυρα={total} σωστά={correct} λάθος={wrong} ποσοστό_σωστών={pct_correct:.1}% μέση_conf_στα_λάθη={avg_wrong_conf:.4} ορια_παραχθέντα={actual_boundaries} ορια_αναμενόμενα={expected_boundaries}\n"
    );
}

fn main() {
    run_case("1 ΣΚΕΤΗ ΑΦΗΓΗΣΗ", "test1_narration_only.wav", GT::AllSpeech, 0);
    run_case("2 ΣΚΕΤΟ BED", "test2_bed_only.wav", GT::AllMusic, 0);
    run_case("3 ΑΦΗΓΗΣΗ+BED -20dB", "test3_narration_bed_m20dB.wav", GT::AllSpeech, 0);
    run_case("4 ΑΦΗΓΗΣΗ+BED -12dB", "test4_narration_bed_m12dB.wav", GT::AllSpeech, 0);
    run_case("5 ΕΝΑΛΛΑΓΗ 30/60/90", "test5_alternation_30_60_90.wav", GT::Alternating, 3);
    run_case("6 BED ΜΠΑΙΝΕΙ ΣΤΑ 60s", "test6_bed_enters_60s.wav", GT::AllSpeech, 0);
}
