//! MEASURE 2026-09-16: ο κανόνας Β (και Γ, Δ, για πλαίσιο) στα εννιά
//! πραγματικά librivox-hq αρχεία. Read-only, ΜΗΔΕΝ αλλαγή στην
//! παραγωγή. ΜΗΔΕΝ γνωστή απάντηση εδώ — ΔΕΝ μετριέται ορθότητα, μόνο
//! δομικά μεγέθη (τμήματα, resets, flagged, <5s).
//!
//! Decode: pass0_decode_to_dump (ΙΔΙΟ με escalation_analysis.rs, F-107)
//! — αυτούσιο production decode path. Ο dump διαβάζεται απευθείας ως
//! raw interleaved f32 LE στερεοφωνικό (ΙΔΙΟ format με DumpSeekProvider,
//! decode_provider.rs:297-303 — [L 4 bytes][R 4 bytes] ανά frame,
//! 48kHz), όχι μέσω hound/WAV.
//!
//! Κανόνας Α: το ΠΡΑΓΜΑΤΙΚΟ smooth_and_segment πάνω στις ΠΡΑΓΜΑΤΙΚΕΣ
//! αποφάσεις (compute_scout_decision) — ΙΔΙΑ boundaries με την
//! παραγωγή/F-107, καμία προσέγγιση.
//! Κανόνες Β/Γ/Δ: δεν έχουν φυσικό confidence δικό τους — για τα
//! ΔΙΚΑ ΤΟΥΣ όρια συντίθεται ScoutDecision{leaning:1.0/0.0,
//! confidence:1.0} (άμεση απόκριση, καμία εξομάλυνση, όπως στο
//! fixture_battery.rs). Για το flagged ΚΑΘΕ κανόνα (πέρα από το Α),
//! χρησιμοποιείται το ΠΡΑΓΜΑΤΙΚΟ avg leaning/confidence (από την
//! αληθινή compute_scout_decision, F-041 κατώφλια) πάνω στα παράθυρα
//! ΤΟΥ τμήματος που ΑΥΤΟΣ ο κανόνας παρήγαγε — «αν κόβαμε εδώ, θα
//! κλιμάκωνε το πραγματικό όργανο αυτό το κομμάτι;».
//!
//! ΧΡΗΣΗ: cargo run --release --bin real_files_battery

use lineos_corpus::scout::{
    compute_cepstral_flux, compute_scout_decision, smooth_and_segment, ScoutDecision, SegmentBoundary,
    SegmentType,
};
use sp314_dsp::analysis::scout::SegmentScout;

const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const MUSIC_CV: f32 = 0.4375;
const SPEECH_CV: f32 = 0.6855;
const MUSIC_FLUX: f32 = 1.3387;
const SPEECH_FLUX: f32 = 1.7242;
const MID_CV: f32 = (MUSIC_CV + SPEECH_CV) / 2.0;
const MID_FLUX: f32 = (MUSIC_FLUX + SPEECH_FLUX) / 2.0;
// ΑΥΤΟΥΣΙΑ scout.rs:235-237
const A7_ZONE_LOW: f32 = 0.3;
const A7_ZONE_HIGH: f32 = 0.7;
const A7_MIN_CONF: f32 = 0.4;
const DUMP_FRAME_BYTES: usize = 8; // 2 × f32 LE, decode_provider.rs

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

fn flagged(leaning: f32, conf: f32) -> bool {
    let in_dead_zone = leaning > A7_ZONE_LOW && leaning < A7_ZONE_HIGH;
    let low_confidence = conf < A7_MIN_CONF;
    in_dead_zone && low_confidence
}

fn music_to_speech_resets(segs: &[(f32, f32, SegmentType)]) -> usize {
    segs.windows(2).filter(|w| w[0].2 == SegmentType::Music && w[1].2 == SegmentType::Speech).count()
}

fn under_5s(segs: &[(f32, f32, SegmentType)]) -> usize {
    segs.iter().filter(|(s, e, _)| (e - s) < 5.0).count()
}

/// flagged με ΤΟ ΠΡΑΓΜΑΤΙΚΟ leaning/confidence, μέσος όρος πάνω στα
/// παράθυρα [start,end) του τμήματος.
fn flagged_count_from_real(
    segs: &[(f32, f32, SegmentType)],
    real_windows: &[(f32, f32, f32)], // (start_sec, leaning, confidence) — ΠΡΑΓΜΑΤΙΚΑ, rule A
) -> usize {
    let mut n = 0;
    for &(s, e, _) in segs {
        let in_range: Vec<&(f32, f32, f32)> = real_windows.iter().filter(|(t, _, _)| *t >= s && *t < e).collect();
        if in_range.is_empty() {
            continue;
        }
        let avg_leaning = in_range.iter().map(|(_, l, _)| l).sum::<f32>() / in_range.len() as f32;
        let avg_conf = in_range.iter().map(|(_, _, c)| c).sum::<f32>() / in_range.len() as f32;
        if flagged(avg_leaning, avg_conf) {
            n += 1;
        }
    }
    n
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

fn real_segs(boundaries: &[SegmentBoundary]) -> Vec<(f32, f32, SegmentType)> {
    boundaries.iter().map(|b| (b.start_sec, b.end_sec, b.segment_type)).collect()
}

fn flagged_count_real(boundaries: &[SegmentBoundary]) -> usize {
    boundaries.iter().filter(|b| flagged(b.avg_leaning, b.avg_confidence)).count()
}

fn run(label: &str, path: &str) {
    let dump = format!("/tmp/real_files_battery_{}.raw", label.split_whitespace().next().unwrap_or("x"));
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .unwrap_or_else(|e| panic!("pass0 decode {path}: {e}"));
    let (left, right) = read_dump_stereo(&dump);
    let _ = std::fs::remove_file(&dump);

    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

    let mut scout = SegmentScout::new();
    let mut start = 0usize;

    let mut times = Vec::new();
    let mut real_decisions: Vec<(f32, ScoutDecision)> = Vec::new();
    let mut real_windows: Vec<(f32, f32, f32)> = Vec::new(); // (t, leaning, confidence) — Α, πραγματικό
    let mut verdicts_b = Vec::new();
    let mut verdicts_g = Vec::new();
    let mut verdicts_d = Vec::new();

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
        let meas = scout.measure(mono_slice, cepstral_flux, sample_rate);
        let dec_a = compute_scout_decision(&meas);

        let v_b = if meas.cepstral_flux >= MID_FLUX { SegmentType::Speech } else { SegmentType::Music };
        let v_g = if !meas.cv_ioi.is_nan() && meas.cv_ioi >= MID_CV { SegmentType::Speech } else { SegmentType::Music };
        let v_d = SegmentType::Speech;

        times.push(start_sec);
        real_decisions.push((start_sec, dec_a));
        real_windows.push((start_sec, dec_a.leaning_score, dec_a.confidence));
        verdicts_b.push(v_b);
        verdicts_g.push(v_g);
        verdicts_d.push(v_d);

        start += hop_samples;
    }

    let real_boundaries = smooth_and_segment(&real_decisions);
    let segs_a = real_segs(&real_boundaries);
    let segs_b = boundaries_from_verdicts(&times, &verdicts_b);
    let segs_g = boundaries_from_verdicts(&times, &verdicts_g);
    let segs_d = boundaries_from_verdicts(&times, &verdicts_d);

    println!("=== {label} ===");
    println!(
        "  {:<10} {:>10} {:>10} {:>10} {:>10}",
        "κανόνας", "τμήματα", "resets(M→S)", "flagged", "<5s"
    );
    println!(
        "  {:<10} {:>10} {:>10} {:>10} {:>10}",
        "Α",
        segs_a.len(),
        music_to_speech_resets(&segs_a),
        flagged_count_real(&real_boundaries),
        under_5s(&segs_a)
    );
    println!(
        "  {:<10} {:>10} {:>10} {:>10} {:>10}",
        "Β",
        segs_b.len(),
        music_to_speech_resets(&segs_b),
        flagged_count_from_real(&segs_b, &real_windows),
        under_5s(&segs_b)
    );
    println!(
        "  {:<10} {:>10} {:>10} {:>10} {:>10}",
        "Γ",
        segs_g.len(),
        music_to_speech_resets(&segs_g),
        flagged_count_from_real(&segs_g, &real_windows),
        under_5s(&segs_g)
    );
    println!(
        "  {:<10} {:>10} {:>10} {:>10} {:>10}",
        "Δ",
        segs_d.len(),
        music_to_speech_resets(&segs_d),
        flagged_count_from_real(&segs_d, &real_windows),
        under_5s(&segs_d)
    );
    println!();
}

fn main() {
    let dir = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    let files = [
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
    println!("MID_CV={MID_CV:.4} MID_FLUX={MID_FLUX:.4}\n");
    for (label, fname) in files {
        let path = format!("{dir}/{fname}");
        run(label, &path);
    }
}
