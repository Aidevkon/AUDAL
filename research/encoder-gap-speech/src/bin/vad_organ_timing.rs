//! MEASURE 2026-09-17: κόστος χρόνου των δύο ζωντανών οργάνων φωνής
//! (Phi2Sensor, VadClassifier<FixedPriors>) — μόνο ο υπολογισμός
//! τους, ΕΞΩ από την παραγωγή, ΧΩΡΙΣ NMF. Τρία τρεξίματα με και
//! χωρίς, στο dracula — ίδιο σχήμα με flatness_rule_timing.rs.
//! ΧΡΗΣΗ: cargo run --release --bin vad_organ_timing -- <dump_path>
use sp314_dsp::analysis::phi1_sensor::{Phi2Pcen, Phi2Sensor, Phi2StreamingFrontend};
use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{FixedPriors, VadClassifier};
use std::time::Instant;

const DUMP_FRAME_BYTES: usize = 8;
const CHUNK: usize = 4096;

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

fn run_fixed_priors(mono: &[f32], left: &[f32], right: &[f32]) -> usize {
    let mut extractor = VadFeatureExtractor::new();
    let mut classifier = VadClassifier::new(FixedPriors);
    let features = extractor.process_chunk(mono, left, right);
    let mut n = 0;
    for f in &features {
        let _ = classifier.process(f, -70.0);
        n += 1;
    }
    n
}

fn run_phi2(mono: &[f32]) -> usize {
    let mut frontend = Phi2StreamingFrontend::new();
    let mut pcen = Phi2Pcen::new();
    let mut sensor = Phi2Sensor::new();
    let mut n = 0;
    let mut pos = 0;
    while pos < mono.len() {
        let end = (pos + CHUNK).min(mono.len());
        for mel in frontend.push(&mono[pos..end]) {
            let p = pcen.process(&mel);
            if sensor.push_frame(&p).is_some() { n += 1; }
        }
        pos = end;
    }
    for mel in frontend.finish() {
        let p = pcen.process(&mel);
        if sensor.push_frame(&p).is_some() { n += 1; }
    }
    n
}

/// ΗΔΗ ΜΕΤΡΗΜΕΝΟ 16/09 (flatness_rule_timing.rs, real run_trunk_pass,
/// ΧΩΡΙΣ κανένα όργανο φωνής): μέσος όρος τριών τρεξιμάτων στο ΙΔΙΟ
/// dracula dump. Επαναχρησιμοποιείται εδώ ως ο ΙΔΙΟΣ παρονομαστής
/// σύγκρισης με τα +70.3%/+33% — ΔΕΝ ξαναμετριέται το run_trunk_pass
/// εδώ (θα χρειαζόταν προσωρινό patch στην παραγωγή, ΒΗΜΑ 0.1 έδειξε
/// ότι δεν χρειάζεται για τα posterior — το ΙΔΙΟ ισχύει για το κόστος:
/// το όργανο μετριέται ΜΟΝΟ του, εκτός παραγωγής, και το ποσοστό
/// εκφράζεται πάνω στον ήδη γνωστό παρονομαστή).
const RUN_TRUNK_PASS_BASELINE_SECS: f64 = 43.976;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dump_path = args.get(1).cloned().unwrap_or_else(|| "/tmp/before_dracula.raw".to_string());
    let (left, right) = read_dump_stereo(&dump_path);
    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
    println!("dump={dump_path}  δείγματα={}", mono.len());
    println!("Παρονομαστής σύγκρισης (run_trunk_pass, ΧΩΡΙΣ όργανο, 16/09): {:.3}s\n", RUN_TRUNK_PASS_BASELINE_SECS);

    println!("--- FixedPriors (VadFeatureExtractor + VadClassifier) ---");
    let mut times_fp = Vec::new();
    for run in 0..3 {
        let t0 = Instant::now();
        let n = run_fixed_priors(&mono, &left, &right);
        let elapsed = t0.elapsed().as_secs_f64();
        times_fp.push(elapsed);
        println!("  run {}: {:.3}s ({n} πλαίσια)", run + 1, elapsed);
    }
    println!("  μέσος όρος: {:.3}s", times_fp.iter().sum::<f64>() / 3.0);

    println!("--- Phi2Sensor (Phi2StreamingFrontend + Phi2Pcen + Phi2Sensor) ---");
    let mut times_ph = Vec::new();
    for run in 0..3 {
        let t0 = Instant::now();
        let n = run_phi2(&mono);
        let elapsed = t0.elapsed().as_secs_f64();
        times_ph.push(elapsed);
        println!("  run {}: {:.3}s ({n} πλαίσια)", run + 1, elapsed);
    }
    println!("  μέσος όρος: {:.3}s", times_ph.iter().sum::<f64>() / 3.0);

    let fp = times_fp.iter().sum::<f64>() / 3.0;
    let ph = times_ph.iter().sum::<f64>() / 3.0;
    println!("\nΑν προστεθεί στο run_trunk_pass (παρονομαστής {:.3}s):", RUN_TRUNK_PASS_BASELINE_SECS);
    println!("  FixedPriors: +{:.3}s ⇒ {:+.1}%", fp, 100.0 * fp / RUN_TRUNK_PASS_BASELINE_SECS);
    println!("  Phi2Sensor:  +{:.3}s ⇒ {:+.1}%", ph, 100.0 * ph / RUN_TRUNK_PASS_BASELINE_SECS);
}
