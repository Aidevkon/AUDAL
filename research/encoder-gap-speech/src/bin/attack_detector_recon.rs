//! RECON 2026-09-16 (ΣΥΜΠΛΗΡΩΜΑ στη μέτρηση διακύμανσης RMS): πόσες
//! επιθέσεις βρήκε ο ανιχνευτής, και τι έδωσαν τα δύο ωμά
//! χαρακτηριστικά (cv_ioi, cepstral_flux) πριν το z-score. Read-only,
//! ΜΗΔΕΝ αλλαγή στο κατώφλι του ανιχνευτή ή στα centroids.
//!
//! ΑΝΤΙΓΡΑΦΟ, γραμμή-προς-γραμμή, του SegmentScout::measure
//! (sp314-dsp/src/analysis/scout.rs:32-72) — η μόνη προσθήκη είναι η
//! επιστροφή του onsets.len()/iois.len() που η παραγωγή υπολογίζει
//! ήδη αλλά πετάει (μόνο το τελικό cv_ioi ταξιδεύει σε
//! ScoutMeasurements). SpectralFluxDetector::detect (stft/
//! spectral_flux.rs:29-82) ΕΠΑΛΗΘΕΥΤΗΚΕ καθαρή συνάρτηση του δοθέντος
//! slice — `prev`/`flux`/`beats` όλα τοπικά Vecs, ΜΗΔΕΝ state που
//! διαρρέει ανάμεσα σε κλήσεις (το μόνο persistent πεδίο είναι το
//! StftEngine, reuse για απόδοση, όχι για σημασιολογία) — ασφαλές να
//! κληθεί σε ξεχωριστό instance εδώ, ίδιο αποτέλεσμα με την παραγωγή.
//! Threshold 0.20 ΑΥΤΟΥΣΙΟ (scout.rs:28).
//!
//! Πιστότητα ελεγμένη: cv_ioi/cepstral_flux του αντιγράφου
//! συγκρίνονται bit-for-bit με το πραγματικό SegmentScout::measure σε
//! ΚΑΘΕ παράθυρο.
//!
//! Ξαναχρησιμοποιεί τα έξι WAV του MEASURE 16/09
//! (/tmp/scout-groundtruth/test*.wav) — ΔΕΝ παράγει νέο ήχο.
//!
//! ΧΡΗΣΗ: cargo run --release --bin attack_detector_recon

use lineos_corpus::scout::{compute_cepstral_flux, compute_scout_decision, ScoutMeasurements};
use sp314_dsp::analysis::scout::SegmentScout;
use sp314_dsp::stft::{SpectralFluxDetector, HOP_SIZE};

const WORK: &str = "/tmp/scout-groundtruth";
const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;

// ΑΥΤΟΥΣΙΑ κέντρα, lineos-corpus/src/scout.rs:47,52 (F-041 Evaluated Centroids)
const MUSIC_CV: f32 = 0.4375;
const SPEECH_CV: f32 = 0.6855;
const MUSIC_FLUX: f32 = 1.3387;
const SPEECH_FLUX: f32 = 1.7242;

struct AttackDiag {
    onsets: usize,
    iois: usize,
    cv_ioi: f32,       // NaN αν onsets<3 ή mean_ioi<=1e-8 — ΙΔΙΟΣ φρουρός με scout.rs:42,56
    cepstral_flux: f32,
}

/// ΑΝΤΙΓΡΑΦΟ scout.rs:32-72, με έκθεση onsets.len()/iois.len().
fn measure_diag(mono: &[f32], cepstral_flux: f32, sample_rate: u32) -> AttackDiag {
    let mut flux_det = SpectralFluxDetector::new(0.20_f32);
    let mut cv_ioi = f32::NAN;
    let (_, onsets) = flux_det.detect(mono);
    let mut iois_len = 0usize;

    // Guard: Need at least 3 onsets for 2 IOIs to calculate variance. (scout.rs:41-42)
    if onsets.len() >= 3 {
        let hop_size = HOP_SIZE as f32;
        let mut iois = Vec::with_capacity(onsets.len() - 1);
        for i in 1..onsets.len() {
            let diff_frames = (onsets[i] - onsets[i - 1]) as f32;
            let ioi_ms = diff_frames * hop_size * 1000.0 / (sample_rate as f32);
            iois.push(ioi_ms);
        }
        iois_len = iois.len();
        let mean_ioi = iois.iter().sum::<f32>() / (iois.len() as f32);
        // Guard against div-by-zero (scout.rs:55-56)
        if mean_ioi > 1e-8 {
            let mut sum_sq = 0.0;
            for &ioi in &iois {
                let diff = ioi - mean_ioi;
                sum_sq += diff * diff;
            }
            let variance = sum_sq / (iois.len() as f32);
            cv_ioi = libm::sqrtf(variance) / mean_ioi;
        }
    }

    AttackDiag { onsets: onsets.len(), iois: iois_len, cv_ioi, cepstral_flux }
}

fn read_wav_stereo_f32(path: &str) -> (Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let samples: Vec<f32> = reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
    let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
    (left, right)
}

fn median(xs: &mut Vec<f64>) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = xs.len();
    if n % 2 == 0 { (xs[n / 2 - 1] + xs[n / 2]) / 2.0 } else { xs[n / 2] }
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

    println!(
        "ΚΕΝΤΡΑ (F-041 Evaluated Centroids, scout.rs:47,52): MUSIC_CV={MUSIC_CV} SPEECH_CV={SPEECH_CV} MUSIC_FLUX={MUSIC_FLUX} SPEECH_FLUX={SPEECH_FLUX}\n"
    );

    let mut fidelity_mismatches = 0usize;
    let mut nan_guard_total = 0usize;
    let mut nan_guard_wrong = 0usize;

    for (label, wav, gt) in cases {
        let path = format!("{WORK}/{wav}");
        let (left, right) = read_wav_stereo_f32(&path);
        let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();

        let sample_rate = 48000u32;
        let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
        let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

        let mut scout = SegmentScout::new(); // πραγματικό, για fidelity cross-check
        let mut start = 0usize;

        let mut onsets_correct: Vec<f64> = Vec::new();
        let mut onsets_wrong: Vec<f64> = Vec::new();
        let mut cv_correct: Vec<f64> = Vec::new();
        let mut cv_wrong: Vec<f64> = Vec::new();
        let mut flux_correct: Vec<f64> = Vec::new();
        let mut flux_wrong: Vec<f64> = Vec::new();
        let mut rows: Vec<(f32, usize, usize, f32, f32, bool, bool)> = Vec::new();

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

            let diag = measure_diag(mono_slice, cepstral_flux, sample_rate);

            // πιστότητα: ΙΔΙΟ ScoutMeasurements με την πραγματική SegmentScout::measure
            let real_meas = scout.measure(mono_slice, cepstral_flux, sample_rate);
            let diag_meas = ScoutMeasurements { cv_ioi: diag.cv_ioi, cepstral_flux: diag.cepstral_flux };
            let bits_match = (real_meas.cv_ioi.to_bits() == diag_meas.cv_ioi.to_bits())
                || (real_meas.cv_ioi.is_nan() && diag_meas.cv_ioi.is_nan());
            if !bits_match || real_meas.cepstral_flux.to_bits() != diag_meas.cepstral_flux.to_bits() {
                fidelity_mismatches += 1;
            }

            let dec = compute_scout_decision(&real_meas);
            let raw_verdict_speech = dec.leaning_score >= 0.5;
            let center = start_sec + WINDOW_SECS / 2.0;
            let expected = expected_speech(gt, center);
            let is_correct = raw_verdict_speech == expected;
            let hit_nan_guard = diag.cv_ioi.is_nan();

            if hit_nan_guard {
                nan_guard_total += 1;
                if !is_correct {
                    nan_guard_wrong += 1;
                }
            }

            if is_correct {
                onsets_correct.push(diag.onsets as f64);
                if !diag.cv_ioi.is_nan() {
                    cv_correct.push(diag.cv_ioi as f64);
                }
                flux_correct.push(diag.cepstral_flux as f64);
            } else {
                onsets_wrong.push(diag.onsets as f64);
                if !diag.cv_ioi.is_nan() {
                    cv_wrong.push(diag.cv_ioi as f64);
                }
                flux_wrong.push(diag.cepstral_flux as f64);
            }

            rows.push((start_sec, diag.onsets, diag.iois, diag.cv_ioi, diag.cepstral_flux, is_correct, hit_nan_guard));
            start += hop_samples;
        }

        println!("=== {label} ({wav}) ===");
        println!(
            "  σωστά (n={}): επιθέσεις διάμεσος={:.1} cv_ioi διάμεσος={:.4} (n non-NaN={}) flux διάμεσος={:.4}",
            onsets_correct.len(),
            median(&mut onsets_correct.clone()),
            median(&mut cv_correct.clone()),
            cv_correct.len(),
            median(&mut flux_correct.clone())
        );
        println!(
            "  λάθη  (n={}): επιθέσεις διάμεσος={:.1} cv_ioi διάμεσος={:.4} (n non-NaN={}) flux διάμεσος={:.4}",
            onsets_wrong.len(),
            median(&mut onsets_wrong.clone()),
            median(&mut cv_wrong.clone()),
            cv_wrong.len(),
            median(&mut flux_wrong.clone())
        );
        let nan_here = rows.iter().filter(|r| r.6).count();
        let nan_here_wrong = rows.iter().filter(|r| r.6 && !r.5).count();
        println!("  παράθυρα με cv_ioi=NaN (<3 επιθέσεις ή mean_ioi<=1e-8): {nan_here} (στα λάθη: {nan_here_wrong})");
        println!("  --- ανά παράθυρο (t, επιθέσεις, iois, cv_ioi, flux, σωστά;, NaN-φρουρός;) ---");
        for (t, onsets, iois, cv, flux, ok, nan_hit) in &rows {
            println!(
                "  {t:>7.2}s  επιθ={onsets:>3}  ioi={iois:>3}  cv_ioi={:>8}  flux={flux:>7.4}  {}  {}",
                if cv.is_nan() { "NaN".to_string() } else { format!("{cv:.4}") },
                if *ok { "ναι" } else { "ΟΧΙ" },
                if *nan_hit { "NaN-φρουρός" } else { "" }
            );
        }
        println!();
    }

    println!("ΠΙΣΤΟΤΗΤΑ ΑΝΤΙΓΡΑΦΟΥ: {fidelity_mismatches} αναντιστοιχίες σε cv_ioi/cepstral_flux έναντι της πραγματικής SegmentScout::measure (πρέπει να είναι 0).");
    println!("ΣΥΝΟΛΟ παραθύρων που χτυπάνε τον NaN-φρουρό (σε 696): {nan_guard_total} (στα λάθη: {nan_guard_wrong}).");
}
