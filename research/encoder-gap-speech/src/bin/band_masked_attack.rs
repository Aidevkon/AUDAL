//! MEASURE 2026-09-16: μάσκα στα bins του ανιχνευτή επιθέσεων ΚΑΙ
//! δυναμικό κατώφλι — τέσσερα κελιά, στα δώδεκα δοκίμια με γνωστή
//! απάντηση. Read-only, ΜΗΔΕΝ αλλαγή στην παραγωγή. Όλη η αλλαγή ζει
//! ΕΔΩ, σε αντίγραφο του SpectralFluxDetector::detect
//! (sp314-dsp/src/stft/spectral_flux.rs:29-82) που περιορίζει τα
//! όρια του βρόχου άθροισης flux σε μια ζώνη bins — ΤΙΠΟΤΑ άλλο
//! αλλάζει: το `prev` array μένει πλήρους μεγέθους (N_BINS), το
//! StftEngine/FFT είναι ΤΟ ΠΡΑΓΜΑΤΙΚΟ (sp314_dsp::stft::StftEngine,
//! καμία επανεφεύρεση), μηδέν φάση/καθυστέρηση/μεταβατικό αγγίζεται.
//!
//! ΤΕΣΣΕΡΑ ΚΕΛΙΑ:
//!   ΣΗΜΕΡΙΝΟ  = όλα τα bins (0..N_BINS) + σταθερό κατώφλι 0.20
//!   (α)       = μάσκα ζώνης + σταθερό κατώφλι — ΔΥΟ εκδοχές:
//!               ωμό 0.20, ΚΑΙ 0.20 κανονικοποιημένο ως προς το
//!               πλήθος bins (0.20 × bins_ζώνης/N_BINS) — ώστε η
//!               σύγκριση να μην μπερδεύει «μάσκα» με «συρρίκνωση».
//!   (β)       = όλα τα bins + δυναμικό κατώφλι
//!   (γ)       = μάσκα ζώνης + δυναμικό κατώφλι
//!
//! ΔΥΝΑΜΙΚΟ ΚΑΤΩΦΛΙ, δηλωμένο ΠΡΙΝ το τρέξιμο: διάμεσος(flux_norm) +
//! k·τυπική_απόκλιση(flux_norm), k=2.0, υπολογισμένο ΤΟΠΙΚΑ ανά
//! παράθυρο ανάλυσης 5s (τα flux_norm ΤΟΥ ΙΔΙΟΥ παραθύρου, ~469
//! STFT πλαίσια σε 5s@HOP_SIZE=512) — απλό, όχι κυλιόμενο διαχρονικά,
//! δηλωμένο εξαρχής, όχι συντονισμένο στο αποτέλεσμα.
//!
//! ΔΥΟ ΖΩΝΕΣ ΜΑΣΚΑΣ (FFT_SIZE=2048, N_BINS=1025, bin_hz=23.4375,
//! bin=round(freq_hz·FFT_SIZE/sr)):
//!   ζώνη1 300-3400 Hz  -> bins [13,145]  (133 bins)
//!   ζώνη2 200-5000 Hz  -> bins [9,213]   (205 bins)
//!
//! Το cepstral_flux/MFCC ring ΔΕΝ αγγίζεται — ξεχωριστή διαδρομή
//! (lineos_corpus::mfcc::MfccAnalyzer), επαληθευμένο στον κώδικα:
//! μηδέν κοινή κατάσταση με το SpectralFluxDetector/StftEngine εδώ.
//!
//! Ξαναχρησιμοποιεί τα δώδεκα WAV του F-110/F-111
//! (/tmp/scout-groundtruth/test*.wav) — ΔΕΝ παράγει νέο ήχο.
//!
//! ΧΡΗΣΗ: cargo run --release --bin band_masked_attack

use lineos_corpus::scout::{compute_cepstral_flux, SegmentType};
use sp314_dsp::stft::spectral_flux::FLUX_MIN_DISTANCE;
use sp314_dsp::stft::{StftEngine, HOP_SIZE, N_BINS};

const WORK: &str = "/tmp/scout-groundtruth";
const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;

const MUSIC_CV: f32 = 0.4375;
const SPEECH_CV: f32 = 0.6855;
const CV_POOLED_STD: f32 = 0.1536;
const MUSIC_FLUX: f32 = 1.3387;
const FLUX_POOLED_STD: f32 = 0.1738;
const DELTA_CV: f32 = 1.61458;
const DELTA_FLUX: f32 = 2.21807;
const DELTA_SQ: f32 = 7.52673;
const Z_CLAMP: f32 = 3.0;

// bins, παράγωγα (βλ. σχόλιο module πάνω)
const ZONE1: (usize, usize) = (13, 145); // 300-3400 Hz
const ZONE2: (usize, usize) = (9, 213); // 200-5000 Hz
const DYNAMIC_K: f32 = 2.0;

#[derive(Clone, Copy)]
enum Threshold {
    Fixed(f32),
    Dynamic(f32), // k
}

fn median_std(xs: &[f32]) -> (f32, f32) {
    let mut sorted = xs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = sorted.len();
    let med = if n == 0 {
        0.0
    } else if n % 2 == 0 {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    };
    let mean = xs.iter().sum::<f32>() / n.max(1) as f32;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n.max(1) as f32;
    (med, var.sqrt())
}

/// ΑΝΤΙΓΡΑΦΟ spectral_flux.rs:29-82. ΜΟΝΗ ΑΛΛΑΓΗ: τα όρια του βρόχου
/// άθροισης (`bin_range`) και το κατώφλι peak-picking (`threshold`).
/// prev μένει N_BINS (πλήρες), μόνο ο βρόχος στενεύει.
fn detect_masked(engine: &mut StftEngine, signal: &[f32], bin_range: Option<(usize, usize)>, threshold: Threshold) -> Vec<usize> {
    let (frames, n_frames) = engine.forward(signal);
    let mut flux = vec![0.0_f32; n_frames];
    let mut prev = vec![0.0_f32; N_BINS];
    let (lo, hi) = bin_range.unwrap_or((0, N_BINS - 1));

    for (t, frame) in frames.iter().enumerate() {
        let mut frame_flux = 0.0_f32;
        for b in lo..=hi {
            let mag = (frame[b].re * frame[b].re + frame[b].im * frame[b].im).sqrt();
            let diff = mag - prev[b];
            if diff > 0.0_f32 {
                frame_flux += diff;
            }
            prev[b] = mag;
        }
        flux[t] = frame_flux;
    }

    let flux_max = flux.iter().cloned().fold(0.0_f32, f32::max);
    let mut flux_norm = vec![0.0_f32; n_frames];
    if flux_max > 1e-8_f32 {
        for t in 0..n_frames {
            flux_norm[t] = flux[t] / flux_max;
        }
    }

    let thr = match threshold {
        Threshold::Fixed(t) => t,
        Threshold::Dynamic(k) => {
            let (med, std) = median_std(&flux_norm);
            med + k * std
        }
    };

    let mut beats = Vec::new();
    for t in 1..n_frames.saturating_sub(1) {
        if flux_norm[t] >= thr && flux_norm[t] > flux_norm[t - 1] && flux_norm[t] > flux_norm[t + 1] {
            let dist = if beats.is_empty() { FLUX_MIN_DISTANCE + 1 } else { t - *beats.last().unwrap() };
            if dist >= FLUX_MIN_DISTANCE {
                beats.push(t);
            }
        }
    }
    beats
}

fn cv_ioi_from_onsets(onsets: &[usize], sample_rate: u32) -> f32 {
    if onsets.len() < 3 {
        return f32::NAN;
    }
    let hop_size = HOP_SIZE as f32;
    let mut iois = Vec::with_capacity(onsets.len() - 1);
    for i in 1..onsets.len() {
        let diff_frames = (onsets[i] - onsets[i - 1]) as f32;
        iois.push(diff_frames * hop_size * 1000.0 / sample_rate as f32);
    }
    let mean_ioi = iois.iter().sum::<f32>() / iois.len() as f32;
    if mean_ioi <= 1e-8 {
        return f32::NAN;
    }
    let mut sum_sq = 0.0;
    for &ioi in &iois {
        let diff = ioi - mean_ioi;
        sum_sq += diff * diff;
    }
    let variance = sum_sq / iois.len() as f32;
    libm::sqrtf(variance) / mean_ioi
}

fn projection_verdict(cv_ioi: f32, flux: f32) -> SegmentType {
    if cv_ioi.is_nan() || flux.is_nan() {
        // ΑΥΤΟΥΣΙΟ scout.rs:81-90: NaN-φρουρός -> leaning_score=0.5 -> 0.5>=0.5 -> Speech.
        return SegmentType::Speech;
    }
    let p_cv = ((cv_ioi - MUSIC_CV) / CV_POOLED_STD).clamp(-Z_CLAMP, Z_CLAMP);
    let p_flux = ((flux - MUSIC_FLUX) / FLUX_POOLED_STD).clamp(-Z_CLAMP, Z_CLAMP);
    let t = (p_cv * DELTA_CV + p_flux * DELTA_FLUX) / DELTA_SQ;
    let leaning = t.clamp(0.0, 1.0);
    if leaning >= 0.5 { SegmentType::Speech } else { SegmentType::Music }
}

fn read_wav_stereo_f32(path: &str) -> (Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let samples: Vec<f32> = reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
    let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
    (left, right)
}

fn median(xs: &[f32]) -> f32 {
    let mut v: Vec<f32> = xs.iter().cloned().filter(|x| !x.is_nan()).collect();
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 0 { (v[n / 2 - 1] + v[n / 2]) / 2.0 } else { v[n / 2] }
}

fn expected_type(schedule: &[(f32, f32, SegmentType)], center: f32) -> SegmentType {
    for &(s, e, ty) in schedule {
        if center >= s && center < e {
            return ty;
        }
    }
    schedule.last().unwrap().2
}

struct Fixture {
    label: &'static str,
    wav: &'static str,
    schedule: Vec<(f32, f32, SegmentType)>,
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

    let z1_bins = (ZONE1.1 - ZONE1.0 + 1) as f32;
    let z2_bins = (ZONE2.1 - ZONE2.0 + 1) as f32;
    let n_bins_f = N_BINS as f32;
    let thr_z1_norm = 0.20 * z1_bins / n_bins_f;
    let thr_z2_norm = 0.20 * z2_bins / n_bins_f;

    println!("ΖΩΝΗ1 300-3400Hz -> bins [{},{}] ({} bins)  norm-thr={:.5}", ZONE1.0, ZONE1.1, z1_bins as usize, thr_z1_norm);
    println!("ΖΩΝΗ2 200-5000Hz -> bins [{},{}] ({} bins)  norm-thr={:.5}", ZONE2.0, ZONE2.1, z2_bins as usize, thr_z2_norm);
    println!("ΔΥΝΑΜΙΚΟ ΚΑΤΩΦΛΙ: median(flux_norm) + {DYNAMIC_K}*std(flux_norm), ανά παράθυρο 5s\n");

    let configs: Vec<(&str, Option<(usize, usize)>, Threshold)> = vec![
        ("ΣΗΜΕΡΙΝΟ (όλα+0.20)", None, Threshold::Fixed(0.20)),
        ("α-ζ1-ωμό (μάσκα1+0.20)", Some(ZONE1), Threshold::Fixed(0.20)),
        ("α-ζ1-καν (μάσκα1+0.20×bins/N)", Some(ZONE1), Threshold::Fixed(thr_z1_norm)),
        ("α-ζ2-ωμό (μάσκα2+0.20)", Some(ZONE2), Threshold::Fixed(0.20)),
        ("α-ζ2-καν (μάσκα2+0.20×bins/N)", Some(ZONE2), Threshold::Fixed(thr_z2_norm)),
        ("β (όλα+δυναμικό)", None, Threshold::Dynamic(DYNAMIC_K)),
        ("γ-ζ1 (μάσκα1+δυναμικό)", Some(ZONE1), Threshold::Dynamic(DYNAMIC_K)),
        ("γ-ζ2 (μάσκα2+δυναμικό)", Some(ZONE2), Threshold::Dynamic(DYNAMIC_K)),
    ];

    for fx in &fixtures {
        let path = format!("{WORK}/{}", fx.wav);
        let (left, right) = read_wav_stereo_f32(&path);
        let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
        let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
        let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
        let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

        // ΩΜΟ cepstral_flux ανά παράθυρο, ΞΕΧΩΡΙΣΤΗ διαδρομή (MFCC), ΑΝΕΠΗΡΕΑΣΤΟ.
        let mut flux_per_window = Vec::new();
        let mut start = 0usize;
        while start + win_samples <= mono.len() {
            let end = start + win_samples;
            let mono_slice = &mono[start..end];
            let mut mfcc_analyzer = lineos_corpus::mfcc::MfccAnalyzer::new();
            let mut mfccs = Vec::new();
            let mut f = 0;
            while f + 1024 <= mono_slice.len() {
                mfccs.push(mfcc_analyzer.compute(&mono_slice[f..f + 1024]));
                f += 512;
            }
            flux_per_window.push(compute_cepstral_flux(&mfccs));
            start += hop_samples;
        }

        println!("=== {} ({}) ===", fx.label, fx.wav);
        for (cfg_label, bin_range, threshold) in &configs {
            let mut engine = StftEngine::new();
            let mut attacks_med = Vec::new();
            let mut cv_list = Vec::new();
            let mut correct = 0usize;
            let mut total = 0usize;
            let mut start = 0usize;
            let mut wi = 0usize;
            while start + win_samples <= mono.len() {
                let end = start + win_samples;
                let mono_slice = &mono[start..end];
                let start_sec = start as f32 / sample_rate as f32;
                let onsets = detect_masked(&mut engine, mono_slice, *bin_range, *threshold);
                let cv = cv_ioi_from_onsets(&onsets, sample_rate);
                attacks_med.push(onsets.len() as f32);
                cv_list.push(cv);

                let flux = flux_per_window[wi];
                let center = start_sec + WINDOW_SECS / 2.0;
                let exp = expected_type(&fx.schedule, center);
                let verdict = projection_verdict(cv, flux);
                if verdict == exp {
                    correct += 1;
                }
                total += 1;

                start += hop_samples;
                wi += 1;
            }
            println!(
                "  {cfg_label:<32} επιθέσεις(διάμ)={:>6.1}  cv_ioi(διάμ)={:>7.4}  ορθότητα={:>3}/{total} ({:.1}%)",
                median(&attacks_med), median(&cv_list), correct, 100.0 * correct as f32 / total as f32
            );
        }
        println!();
    }
}
