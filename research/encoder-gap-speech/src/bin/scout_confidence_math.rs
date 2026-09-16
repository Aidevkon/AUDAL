//! RECON 2026-09-16: ο τύπος της σιγουριάς, αυτούσιος. Read-only, ΜΗΔΕΝ
//! αλλαγή στην παραγωγή.
//!
//! Item 1: compute_scout_decision (scout.rs:80-117) χρησιμοποιεί το
//! ΩΜΟ `t` (η ασφράγιστη προβολή, γραμμή 97) και ΟΧΙ το κλαμπαρισμένο
//! `leaning_score` (γραμμή 98) στον υπολογισμό του confidence —
//! βλέπε conf_raw (γρ.101: `libm::fabsf(t - 0.5)`, όχι
//! `leaning_score - 0.5`) ΚΑΙ την προβολή του perpendicular penalty
//! (γρ.105-106: `t * DELTA_CV`/`t * DELTA_FLUX`). Το `t` ΔΕΝ
//! κλαμπάρεται πουθενά στο [0,1] — μόνο ο ΑΠΟΓΟΝΟΣ του
//! (`leaning_score`) κλαμπάρεται, στη γραμμή 98. Το ίδιο το `t`
//! μπορεί να είναι οποιοδήποτε real number (τα p_cv/p_flux
//! κλαμπάρονται στο [-3,3] πριν την προβολή, όχι το `t` μετά).
//!
//! Items 2/3: το production ScoutDecision ΔΕΝ εκθέτει t/d_perp_sq —
//! μόνο leaning_score/confidence (τα ΜΟΝΑ δημόσια πεδία,
//! lineos-corpus/src/scout.rs:20-24). Για να μετρηθούν χρειάζεται
//! ΤΟΠΙΚΟ αντίγραφο του compute_scout_decision που επιστρέφει ΚΑΙ τα
//! δύο — επαληθευμένο γραμμή-προς-γραμμή έναντι scout.rs:80-117 (ΙΔΙΕΣ
//! σταθερές, ΙΔΙΑ σειρά πράξεων) ΚΑΙ ελεγμένο σε ΚΑΘΕ παράθυρο ότι
//! παράγει το ΑΚΡΙΒΩΣ ίδιο leaning_score/confidence με το πραγματικό
//! `lineos_corpus::scout::compute_scout_decision` (assert bit-for-bit
//! παρακάτω) — η μόνη προσθήκη είναι η επιστροφή του t/d_perp_sq που
//! η παραγωγή υπολογίζει ήδη αλλά πετάει.
//!
//! Ξαναχρησιμοποιεί τα έξι WAV του προηγούμενου MEASURE
//! (/tmp/scout-groundtruth/test*.wav) — ΔΕΝ παράγει νέο ήχο.
//!
//! ΧΡΗΣΗ: cargo run --release --bin scout_confidence_math

use lineos_corpus::scout::{compute_scout_decision as production_decision, ScoutMeasurements};
use sp314_dsp::analysis::scout::SegmentScout;

const WORK: &str = "/tmp/scout-groundtruth";
const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;

// ΑΥΤΟΥΣΙΕΣ από lineos-corpus/src/scout.rs:47-78 (ιδιωτικές εκεί, εδώ
// αντίγραφο για να εκτεθεί το t/d_perp_sq που η παραγωγή πετάει).
const MUSIC_CV: f32 = 0.4375;
const CV_POOLED_STD: f32 = 0.1536;
const MUSIC_FLUX: f32 = 1.3387;
const FLUX_POOLED_STD: f32 = 0.1738;
const DELTA_CV: f32 = 1.61458;
const DELTA_FLUX: f32 = 2.21807;
const DELTA_SQ: f32 = 7.52673;
const Z_CLAMP: f32 = 3.0;
const PERP_R_SQ: f32 = 9.0;

struct Diag {
    leaning_score: f32,
    confidence: f32,
    t_raw: f32,
    d_perp_sq: f32,
}

/// ΑΝΤΙΓΡΑΦΟ, γραμμή-προς-γραμμή, του compute_scout_decision
/// (scout.rs:80-117) — μόνη διαφορά: επιστρέφει ΚΑΙ t_raw/d_perp_sq.
fn compute_scout_decision_diag(m: &ScoutMeasurements) -> Diag {
    if m.cv_ioi.is_nan() || m.cepstral_flux.is_nan() || m.cv_ioi.is_infinite() || m.cepstral_flux.is_infinite() {
        return Diag { leaning_score: 0.5, confidence: 0.0, t_raw: f32::NAN, d_perp_sq: f32::NAN };
    }
    let p_cv = ((m.cv_ioi - MUSIC_CV) / CV_POOLED_STD).clamp(-Z_CLAMP, Z_CLAMP);
    let p_flux = ((m.cepstral_flux - MUSIC_FLUX) / FLUX_POOLED_STD).clamp(-Z_CLAMP, Z_CLAMP);
    let t = ((p_cv * DELTA_CV) + (p_flux * DELTA_FLUX)) / DELTA_SQ;
    let leaning_score = t.clamp(0.0, 1.0);
    let conf_raw = (libm::fabsf(t - 0.5) * 2.0).clamp(0.0, 1.0);
    let proj_cv = t * DELTA_CV;
    let proj_flux = t * DELTA_FLUX;
    let perp_cv = p_cv - proj_cv;
    let perp_flux = p_flux - proj_flux;
    let d_perp_sq = perp_cv * perp_cv + perp_flux * perp_flux;
    let penalty = (1.0 - d_perp_sq / PERP_R_SQ).clamp(0.0, 1.0);
    let confidence = conf_raw * penalty;
    Diag { leaning_score, confidence, t_raw: t, d_perp_sq }
}

fn read_wav_stereo_f32(path: &str) -> (Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let samples: Vec<f32> = reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
    let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
    (left, right)
}

/// Mirror ΑΚΡΙΒΕΣ του scan_file (scout_scanner.rs:15-68) — ίδιος
/// βρόχος, ίδιο mono/window/hop/MFCC, μόνο η κλήση στο τέλος αλλάζει
/// (diag αντί για production) ΚΑΙ κρατάει και τα δύο για σύγκριση.
fn scan_with_diag(left: &[f32], right: &[f32], sample_rate: u32) -> Vec<(f32, Diag, lineos_corpus::scout::ScoutDecision)> {
    let mono_full: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;
    let mut scout = SegmentScout::new();
    let mut start = 0usize;
    let mut out = Vec::new();
    while start + win_samples <= mono_full.len() {
        let end = start + win_samples;
        let mono_slice = &mono_full[start..end];
        let start_sec = start as f32 / sample_rate as f32;

        let mut mfcc_analyzer = lineos_corpus::mfcc::MfccAnalyzer::new();
        let mut mfccs = Vec::new();
        let mut f = 0;
        while f + 1024 <= mono_slice.len() {
            mfccs.push(mfcc_analyzer.compute(&mono_slice[f..f + 1024]));
            f += 512;
        }
        let cepstral_flux = lineos_corpus::scout::compute_cepstral_flux(&mfccs);
        let meas = scout.measure(mono_slice, cepstral_flux, sample_rate);

        let diag = compute_scout_decision_diag(&meas);
        let prod = production_decision(&meas);
        out.push((start_sec, diag, prod));
        start += hop_samples;
    }
    out
}

fn main() {
    println!("=== ITEM 1 — compute_scout_decision, ΑΥΤΟΥΣΙΟ (scout.rs:80-117) ===\n");
    println!("{}", include_str!("../../../../lineos/m1/lineos-corpus/src/scout.rs")
        .lines()
        .skip(79)
        .take(38)
        .collect::<Vec<_>>()
        .join("\n"));

    println!("\n\n=== ITEM 2/3 — t_raw και d_perp_sq ανά παράθυρο, έξι δοκίμια ===\n");

    let cases = [
        ("1 ΣΚΕΤΗ ΑΦΗΓΗΣΗ", "test1_narration_only.wav"),
        ("2 ΣΚΕΤΟ BED", "test2_bed_only.wav"),
        ("3 ΑΦΗΓΗΣΗ+BED -20dB", "test3_narration_bed_m20dB.wav"),
        ("4 ΑΦΗΓΗΣΗ+BED -12dB", "test4_narration_bed_m12dB.wav"),
        ("5 ΕΝΑΛΛΑΓΗ 30/60/90", "test5_alternation_30_60_90.wav"),
        ("6 BED ΜΠΑΙΝΕΙ ΣΤΑ 60s", "test6_bed_enters_60s.wav"),
    ];

    // ΙΔΙΑ ground truth σύμβαση με το scout_groundtruth.rs (κέντρο παραθύρου)
    let expected_speech = |label: &str, center: f32| -> bool {
        match label {
            "2 ΣΚΕΤΟ BED" => false,
            "5 ΕΝΑΛΛΑΓΗ 30/60/90" => {
                let block = (center / 30.0).floor() as i64;
                block % 2 == 0
            }
            _ => true, // 1, 3, 4, 6: πάντα Speech εκ κατασκευής
        }
    };

    let mut fidelity_mismatches = 0usize;
    for (label, wav) in cases {
        let path = format!("{WORK}/{wav}");
        let (left, right) = read_wav_stereo_f32(&path);
        let rows = scan_with_diag(&left, &right, lineos_types::analysis::ANALYSIS_SAMPLE_RATE);

        let mut out_of_range = 0usize;
        let mut out_of_range_wrong = 0usize;
        let mut wrong = 0usize;
        let mut d_perp_wrong_sum = 0.0f64;
        let mut d_perp_correct_sum = 0.0f64;
        let mut n_wrong = 0usize;
        let mut n_correct = 0usize;

        for (t, diag, prod) in &rows {
            // πιστότητα: το αντίγραφο ΠΡΕΠΕΙ να ταιριάζει bit-for-bit με την παραγωγή
            if diag.leaning_score.to_bits() != prod.leaning_score.to_bits()
                || diag.confidence.to_bits() != prod.confidence.to_bits()
            {
                fidelity_mismatches += 1;
            }

            let raw_verdict_is_speech = diag.leaning_score >= 0.5;
            let center = t + WINDOW_SECS / 2.0;
            let expected_is_speech = expected_speech(label, center);
            let is_wrong = raw_verdict_is_speech != expected_is_speech;

            let t_out = diag.t_raw < 0.0 || diag.t_raw > 1.0;
            if t_out {
                out_of_range += 1;
                if is_wrong {
                    out_of_range_wrong += 1;
                }
            }

            if is_wrong {
                wrong += 1;
                n_wrong += 1;
                d_perp_wrong_sum += diag.d_perp_sq as f64;
            } else {
                n_correct += 1;
                d_perp_correct_sum += diag.d_perp_sq as f64;
            }
        }

        let total = rows.len();
        let avg_d_perp_wrong = if n_wrong > 0 { d_perp_wrong_sum / n_wrong as f64 } else { f64::NAN };
        let avg_d_perp_correct = if n_correct > 0 { d_perp_correct_sum / n_correct as f64 } else { f64::NAN };

        println!(
            "{label:<24} παράθυρα={total:<4} t_εξω_[0,1]={out_of_range:<3} (στα_λάθη={out_of_range_wrong:<3}) λάθη={wrong:<4} d_perp_sq_μέσος(λάθη)={avg_d_perp_wrong:>7.4} d_perp_sq_μέσος(σωστά)={avg_d_perp_correct:>7.4}"
        );
    }

    println!("\nΠΙΣΤΟΤΗΤΑ ΑΝΤΙΓΡΑΦΟΥ: {fidelity_mismatches} αναντιστοιχίες σε leaning_score/confidence έναντι της πραγματικής compute_scout_decision (πρέπει να είναι 0).");
    println!("PERP_R_SQ = {PERP_R_SQ} (R={:.1}) — F-041: p90 πραγματικής φωνής=1.577, max=2.285.", PERP_R_SQ.sqrt());
}
