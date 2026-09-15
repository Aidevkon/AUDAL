//! Ο ανιχνευτής βόμβου δικτύου. ΤΟ ΟΡΓΑΝΟ, όχι η πολιτική.
//!
//! Μέθοδος επιλεγμένη με μέτρηση (73db55d): αντι-αναδιπλωτικό ×2 @450Hz →
//! αποδεκατισμός ×48 → ΕΝΑ FFT N=4096 (Δf = sample_rate/48/4096). Στο πιο
//! δύσκολο από τα εφτά δοκίμια (4 dB ζητούμενη προεξοχή) ξεχώρισε τη γραμμή
//! από το φόντο κατά 11.46/13.30 dB, έναντι 4.94/4.50 της εναλλακτικής
//! (Welch N=16384 στα 48kHz, καμία αποδεκάτιση).
//!
//! Το φίλτρο είναι ΥΠΑΡΧΟΝ: `crate::dsp::biquad::butter_lp2_prewarped` —
//! το πραγματικό Butterworth (Q=1/√2, prewarped)· ΟΧΙ το
//! `analysis::pre_analysis::butter_lp2`, που το ίδιο του το σχόλιο ομολογεί
//! Q=1.414 (resonant, mislabeled) και ρητά παραπέμπει σε αυτό εδώ για
//! signal-path χρήση.
//!
//! ⚠ ΤΑ ΟΡΙΑ ΤΟΥ, ΜΕΤΡΗΜΕΝΑ 2026-09-14 σε επτά δοκίμια γνωστής απάντησης
//! (lineos/m1/sp314-dsp/tests/fixtures/hum_detector/, manifest.json's
//! `_decim1k` πεδία, commit 73db55d):
//!   · Η ΠΡΟΕΞΟΧΗ ΔΕΝ ΕΙΝΑΙ ΣΥΓΚΡΙΣΙΜΗ ΜΕ ΜΕΤΡΗΣΗ WELCH ΣΤΑ 48kHz. Το ίδιο
//!     δοκίμιο διαβάζεται 5.12 dB εκεί (17 τμήματα, μέσος όρος) και
//!     16.08 dB εδώ (ένα τμήμα, καμία μέση όρος — η ανάλυση 0.244Hz
//!     αντισταθμίζει). Κάθε κατώφλι πάνω σε αυτόν τον αριθμό πρέπει να
//!     μετρηθεί με ΑΥΤΟ το όργανο, όχι μεταφερμένο από αλλού.
//!   · ΠΡΑΓΜΑΤΙΚΟ ΔΩΜΑΤΙΟ ΜΠΟΡΕΙ ΝΑ ΔΩΣΕΙ 5 dB ΧΩΡΙΣ ΑΝΤΙΛΗΠΤΟ ΒΟΜΒΟ: το
//!     αρνητικό δοκίμιο (μηδέν εγχυμένο) διαβάζει 5.18 dB στα 60Hz με αυτό
//!     το όργανο — πραγματικό, μικρό υπόλειμμα δικτύου στον ίδιο τον
//!     φορέα (mobydick_000_melville.mp3), που το χοντρύτερο πλέγμα Welch
//!     (2.93Hz/κάδο) αραιώνει. Κατώφλι κάτω από αυτό θα δώσει ψευδώς
//!     θετικό εδώ, και το δοκίμιο δεν μπορεί να το πιάσει.
//!
//! ΜΗΔΕΝ ΓΝΩΣΗ ΠΡΟΟΡΙΣΜΟΥ. ΜΗΔΕΝ ΚΑΤΩΦΛΙ «υπάρχει βόμβος». Η πολιτική
//! (πού βρίσκεται η παύση στη ζωντανή διαδρομή, τι κατώφλι σημαίνει
//! «θετικό») ζει στον orchestrator — ΕΞΩ από αυτό το αρχείο, επίτηδες.

use crate::dsp::biquad::butter_lp2_prewarped;
use rustfft::{num_complex::Complex, FftPlanner};

const AA_CUTOFF_HZ: f32 = 450.0;
const AA_SECTIONS: usize = 2; // 4ης τάξης συνολικά· βλ. σχόλιο στο decimate_with_antialias
const DECIM: usize = 48;
const NFFT: usize = 4096;
const SEARCH_LO_HZ: f32 = 40.0;
const SEARCH_HI_HZ: f32 = 75.0;

/// Η γραμμή που βρέθηκε στη ζώνη αναζήτησης — δεν είναι ετυμηγορία.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MainsLine {
    pub hz: f32,
    pub prominence_db: f32,
}

/// Ψάχνει ζώνη 40-75Hz μέσα στα δοσμένα δείγματα (ήδη επιλεγμένη παύση —
/// αυτό το function δεν ξέρει τι είναι παύση, βλ. πολιτική στον orchestrator)
/// για την πιο προεξέχουσα γραμμή. `None` ΜΟΝΟ όταν τα δείγματα είναι πολύ
/// λίγα για να αποδεκατιστούν καν (< 48 δείγματα εισόδου) — ΠΟΤΕ ως κρίση
/// «δεν υπάρχει βόμβος». Μια χαμηλή `prominence_db` σημαίνει ακριβώς αυτό:
/// χαμηλή προεξοχή, μετρημένη — η ερμηνεία είναι του καλούντος.
pub fn detect_mains_line(samples: &[f32], sample_rate: u32) -> Option<MainsLine> {
    detect_band_peak(samples, sample_rate, SEARCH_LO_HZ, SEARCH_HI_HZ)
}

/// ΙΔΙΟ όργανο με `detect_mains_line` (αποδεκατισμός ×48 → ένα FFT
/// N=4096), ζώνη αναζήτησης ως παράμετρος αντί για καρφωμένη — καμία
/// αντιγραφή της αλυσίδας decimate/FFT/peak. `detect_mains_line` είναι
/// τώρα ένα λεπτό περιτύλιγμα γύρω από αυτό, με SEARCH_LO_HZ/SEARCH_HI_HZ.
///
/// ⚠ ΓΙΑ ΤΗ ΧΡΗΣΗ [70,250]Hz (θεμελιώδης ομιλίας, orchestrator): ΔΕΝ ΕΙΝΑΙ
/// ανιχνευτής τονικού ύψους (pitch tracker). Επιστρέφει το ΜΕΓΙΣΤΟ bin
/// της ζώνης — μπορεί να είναι αρμονική, formant, ή θεμελιώδης. Ελέγχθηκε
/// 2026-09-15 (εννέα πραγματικά αρχεία αφήγησης, chat) ότι δεν υπάρχει
/// συστηματική κορυφή στο μισό της βρεθείσας συχνότητας (±2dB από τοπικό
/// μέσο σε όλα) — δεν πιάνει 2η αρμονική αντί για θεμελιώδη σε αυτό το
/// δείγμα. ΔΕΝ αποδεικνύει ταυτοποίηση pitch F0.
pub fn detect_band_peak(samples: &[f32], sample_rate: u32, lo_hz: f32, hi_hz: f32) -> Option<MainsLine> {
    if samples.len() < DECIM {
        return None;
    }
    let decimated = decimate_with_antialias(samples, sample_rate as f32);
    let dec_sr = sample_rate as f32 / DECIM as f32;
    let psd = single_fft_psd_db(&decimated);
    let bin_hz = dec_sr / NFFT as f32;
    peak_in_band(&psd, bin_hz, lo_hz, hi_hz)
        .map(|(hz, prominence_db)| MainsLine { hz, prominence_db })
}

/// Αντι-αναδιπλωτικό: `AA_SECTIONS` × 2ης τάξης Butterworth (prewarped),
/// στα `AA_CUTOFF_HZ`. Λογαριασμός: 2 sections = 4ης τάξης, ~24dB/οκτάβα.
/// Από 450Hz ως το αρχικό Nyquist (sample_rate/2, π.χ. 24kHz στα 48kHz)
/// είναι log2(24000/450) ≈ 5.74 οκτάβες × 24dB/οκτάβα ≈ 138dB εξασθένηση —
/// άφθονο περιθώριο για αποδεκατισμό ×48 σε ένα βήμα.
fn decimate_with_antialias(samples: &[f32], sample_rate: f32) -> Vec<f32> {
    let mut secs: Vec<_> = (0..AA_SECTIONS)
        .map(|_| butter_lp2_prewarped(AA_CUTOFF_HZ, sample_rate))
        .collect();
    samples
        .iter()
        .map(|&s| {
            let mut y = s;
            for sec in secs.iter_mut() {
                y = sec.process(y);
            }
            y
        })
        .step_by(DECIM)
        .collect()
}

/// ΕΝΑ FFT, zero-padded σε NFFT. Hann στα πραγματικά δείγματα, μετά
/// μηδενικά. Ο FftPlanner φτιάχνεται ΜΙΑ φορά — ίδιο ιδίωμα με
/// `stft/mod.rs`, `analysis/phi1_sensor.rs`. Καμία επανάληψη εδώ ώστε να
/// επαναχρησιμοποιηθεί — μία κλήση, ένα plan.
fn single_fft_psd_db(decimated: &[f32]) -> Vec<f32> {
    let n = decimated.len().min(NFFT);
    let win: Vec<f32> = (0..n)
        .map(|i| {
            if n <= 1 {
                1.0
            } else {
                0.5 - 0.5 * (2.0 * core::f32::consts::PI * i as f32 / (n - 1) as f32).cos()
            }
        })
        .collect();
    let wpow: f64 = win.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>().max(1e-30);
    let mut buf: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); NFFT];
    for i in 0..n {
        buf[i] = Complex::new(decimated[i] * win[i], 0.0);
    }
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(NFFT);
    fft.process(&mut buf);
    (0..=NFFT / 2)
        .map(|k| {
            let p = (buf[k].re as f64).powi(2) + (buf[k].im as f64).powi(2);
            let scale = if k == 0 || k == NFFT / 2 { 1.0 } else { 2.0 };
            let m = scale * p / wpow;
            if m < 1e-30 {
                -300.0
            } else {
                (10.0 * m.log10()) as f32
            }
        })
        .collect()
}

/// Διάμεσος ±25Hz γύρω από τον κάδο k, εξαιρώντας ±4Hz — ίδιος ορισμός με
/// `sp314-dsp/tests/fixture_factory.rs`'s `local_floor_db`.
fn local_floor_db(psd: &[f32], k: usize, bin_hz: f32) -> f32 {
    let span = (25.0 / bin_hz) as usize;
    let skip = (4.0 / bin_hz) as usize;
    let lo = k.saturating_sub(span);
    let hi = (k + span).min(psd.len().saturating_sub(1));
    let mut v: Vec<f32> = (lo..=hi)
        .filter(|&i| i < k.saturating_sub(skip) || i > k + skip)
        .map(|i| psd[i])
        .collect();
    if v.is_empty() {
        return psd[k];
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Η πιο ψηλή κορυφή μέσα σε [lo_hz, hi_hz], με παραβολική παρεμβολή, και
/// η προεξοχή της πάνω από το local_floor_db στον ίδιο κάδο.
fn peak_in_band(psd: &[f32], bin_hz: f32, lo_hz: f32, hi_hz: f32) -> Option<(f32, f32)> {
    if psd.len() < 3 {
        return None;
    }
    let k_lo = ((lo_hz / bin_hz).round() as usize).max(1);
    let k_hi = ((hi_hz / bin_hz).round() as usize).min(psd.len() - 2);
    if k_lo > k_hi {
        return None;
    }
    let mut best_k = k_lo;
    let mut best_v = f32::NEG_INFINITY;
    for k in k_lo..=k_hi {
        if psd[k] > best_v {
            best_v = psd[k];
            best_k = k;
        }
    }
    let (ym1, y0, yp1) = (psd[best_k - 1], psd[best_k], psd[best_k + 1]);
    let denom = ym1 - 2.0 * y0 + yp1;
    let delta = if denom.abs() > 1e-9 {
        0.5 * (ym1 - yp1) / denom
    } else {
        0.0
    };
    let hz = (best_k as f32 + delta) * bin_hz;
    let prominence_db = y0 - local_floor_db(psd, best_k, bin_hz);
    Some((hz, prominence_db))
}

// ═══════════════════════════════════════════════════════════════════════
// §0.2 — το κριτήριο παύσης, ΜΙΑ πηγή αντί για δύο (βλ. αναφορά στο chat
// για ποιο τρίτο αντίγραφο ΔΕΝ ενοποιείται και γιατί).
//
// Ίδιος αλγόριθμος με ό,τι ήταν πριν ιδιωτικό, διπλό, σε
// `sp314-dsp/tests/fixture_factory.rs` (otsu_split_bin+longest_pause) ΚΑΙ
// σε `research/encoder-gap-speech/src/bin/hum_spectrum.rs` (longest_pause,
// με εξωτερικό threshold). Και τα δύο αρχεία τώρα καλούν τα δημόσια
// functions παρακάτω — δεν κρατάνε πια δικό τους αντίγραφο.
// ═══════════════════════════════════════════════════════════════════════

const PAUSE_WINDOW_MS: f32 = 100.0;
const PAUSE_HIST_LO_DB: f32 = -100.0;
const PAUSE_HIST_NBINS: usize = 100; // 1 dB/κάδος, -100..0

fn pause_window_rms_db(chunk: &[f32]) -> f32 {
    let e = chunk.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / chunk.len() as f64;
    if e < 1e-20 {
        -200.0
    } else {
        (10.0 * e.log10()) as f32
    }
}

/// Otsu πάνω στην κατανομή RMS/100ms ΟΛΟΥ του δοσμένου buffer. Offline —
/// θέλει όλο το σήμα ήδη στη μνήμη (βλ. §0.1 για γιατί το streaming trunk
/// pass ΔΕΝ μπορεί να καλέσει αυτό χωρίς δεύτερη ανάγνωση ή buffer).
pub fn otsu_pause_threshold_db(samples: &[f32], sample_rate: u32) -> Option<f32> {
    let w = (sample_rate as f32 * PAUSE_WINDOW_MS / 1000.0) as usize;
    if w == 0 {
        return None;
    }
    let windows: Vec<f32> = samples
        .chunks(w)
        .filter(|c| c.len() == w)
        .map(pause_window_rms_db)
        .collect();
    if windows.is_empty() {
        return None;
    }
    let mut hist = [0u32; PAUSE_HIST_NBINS];
    for &v in &windows {
        let b = (v - PAUSE_HIST_LO_DB).floor();
        if b >= 0.0 && (b as usize) < PAUSE_HIST_NBINS {
            hist[b as usize] += 1;
        }
    }
    let t = otsu_split_bin(&hist)?;
    Some(PAUSE_HIST_LO_DB + t as f32 + 0.5)
}

fn otsu_split_bin(hist: &[u32; PAUSE_HIST_NBINS]) -> Option<usize> {
    let total: f64 = hist.iter().map(|&c| c as f64).sum();
    if total == 0.0 {
        return None;
    }
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut w0, mut sum0) = (0.0_f64, 0.0_f64);
    let (mut best_var, mut best_t) = (-1.0_f64, 0usize);
    for t in 0..PAUSE_HIST_NBINS {
        w0 += hist[t] as f64;
        if w0 == 0.0 {
            continue;
        }
        let w1 = total - w0;
        if w1 == 0.0 {
            break;
        }
        sum0 += t as f64 * hist[t] as f64;
        let m0 = sum0 / w0;
        let m1 = (sum_all - sum0) / w1;
        let var = w0 * w1 * (m0 - m1) * (m0 - m1);
        if var > best_var {
            best_var = var;
            best_t = t;
        }
    }
    if best_var < 0.0 {
        None
    } else {
        Some(best_t)
    }
}

/// Ο μεγαλύτερος συνεχόμενος χώρος όπου κάθε παράθυρο 100ms είναι κάτω από
/// `thr_db`. `range_s`: προαιρετικός περιορισμός `[from_s, to_s)` — `None`
/// ψάχνει όλο το `samples`. Επιστρέφει (start_sample, len_samples).
pub fn longest_run_below(
    samples: &[f32],
    sample_rate: u32,
    thr_db: f32,
    range_s: Option<(f32, f32)>,
) -> Option<(usize, usize)> {
    let w = (sample_rate as f32 * PAUSE_WINDOW_MS / 1000.0) as usize;
    if w == 0 {
        return None;
    }
    let sr = sample_rate as f32;
    let (a, b) = match range_s {
        Some((from_s, to_s)) => (
            (from_s * sr) as usize,
            ((to_s * sr) as usize).min(samples.len()),
        ),
        None => (0usize, samples.len()),
    };
    if b <= a + w {
        return None;
    }
    let (mut best, mut cur_start, mut cur_len) = ((0usize, 0usize), a, 0usize);
    let mut i = a;
    while i + w <= b {
        if pause_window_rms_db(&samples[i..i + w]) < thr_db {
            if cur_len == 0 {
                cur_start = i;
            }
            cur_len += w;
        } else {
            if cur_len > best.1 {
                best = (cur_start, cur_len);
            }
            cur_len = 0;
        }
        i += w;
    }
    if cur_len > best.1 {
        best = (cur_start, cur_len);
    }
    if best.1 == 0 {
        None
    } else {
        Some(best)
    }
}

/// Το αντίστροφο κριτήριο του `longest_run_below`: ο μεγαλύτερος
/// συνεχόμενος χώρος όπου ΚΑΘΕ παράθυρο 100ms είναι ΠΑΝΩ από `thr_db` —
/// «μέσα σε ομιλία», όχι «μέσα σε παύση». Ίδιο όργανο (`pause_window_rms_db`,
/// ίδιο παράθυρο), ανεστραμμένη σύγκριση. 2026-09-15, θεμελιώδης ομιλίας.
pub fn longest_run_above(
    samples: &[f32],
    sample_rate: u32,
    thr_db: f32,
    range_s: Option<(f32, f32)>,
) -> Option<(usize, usize)> {
    let w = (sample_rate as f32 * PAUSE_WINDOW_MS / 1000.0) as usize;
    if w == 0 {
        return None;
    }
    let sr = sample_rate as f32;
    let (a, b) = match range_s {
        Some((from_s, to_s)) => (
            (from_s * sr) as usize,
            ((to_s * sr) as usize).min(samples.len()),
        ),
        None => (0usize, samples.len()),
    };
    if b <= a + w {
        return None;
    }
    let (mut best, mut cur_start, mut cur_len) = ((0usize, 0usize), a, 0usize);
    let mut i = a;
    while i + w <= b {
        if pause_window_rms_db(&samples[i..i + w]) >= thr_db {
            if cur_len == 0 {
                cur_start = i;
            }
            cur_len += w;
        } else {
            if cur_len > best.1 {
                best = (cur_start, cur_len);
            }
            cur_len = 0;
        }
        i += w;
    }
    if cur_len > best.1 {
        best = (cur_start, cur_len);
    }
    if best.1 == 0 {
        None
    } else {
        Some(best)
    }
}
