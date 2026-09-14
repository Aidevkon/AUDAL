//! MEASURE: ΠΟΙΑ ΜΕΘΟΔΟΣ ΓΙΑ ΤΟΝ ΑΝΙΧΝΕΥΤΗ ΒΟΜΒΟΥ.
//!
//! Δεν γράφεται ανιχνευτής. Συγκρίνονται δύο μέθοδοι πάνω στα ΙΔΙΑ 7
//! δοκίμια (lineos/m1/sp314-dsp/tests/fixtures/hum_detector/, 6ff5923),
//! ΙΔΙΟ παράθυρο παύσης και για τις δύο (βρίσκεται ΜΙΑ φορά, με το
//! υπάρχον κριτήριο: trunk_pass::run_trunk_pass_with_acx, ίδιο με
//! hum_spectrum.rs).
//!
//! Α — Welch στα 48kHz, N=16384, Hann, 50% επικάλυψη. FftPlanner
//!     HOISTED έξω από τον βρόχο (fixture_factory.rs's welch_psd_db ήδη
//!     το κάνει σωστά· το hum_spectrum.rs's welch() το έφτιαχνε ανά
//!     τμήμα — εδώ διορθώνεται, ώστε η σύγκριση χρόνου να είναι δίκαιη).
//!
//! Β — anti-alias (2× cascaded sp314_dsp::dsp::biquad::
//!     butter_lp2_prewarped στα 450Hz — δηλωμένος λόγος στο ΜΕΤΑ) →
//!     αποδεκατισμός ×48 → 1kHz (ακριβής διαίρεση, μηδέν παρεμβολή
//!     χρειάζεται) → ΕΝΑ FFT N=4096 με zero-padding (Hann στα
//!     πραγματικά δείγματα, μετά padding).
//!
//! Ο ορισμός προεξοχής ΕΙΝΑΙ αυτούσιος από
//! sp314-dsp/tests/fixture_factory.rs's measured_prominence/
//! local_floor_db: psd[k] - διάμεσος(±25Hz, εξαιρώντας ±4Hz), k = ο
//! στρογγυλεμένος κάδος της γνωστής συχνότητας. bin_hz αλλάζει ανά
//! μέθοδο, ο ορισμός όχι.
//!
//! ═══ Η ΠΡΟΒΛΕΨΗ, ΓΡΑΜΜΕΝΗ ΠΡΙΝ ΤΡΕΞΕΙ ΟΤΙΔΗΠΟΤΕ ═══
//! p21, p12: και οι δύο μέθοδοι τα βρίσκουν καθαρά.
//! p04: η Β ξεχωρίζει καθαρότερα — 12× καλύτερη ανάλυση συγκεντρώνει τη
//!      γραμμή σε έναν κάδο αντί για τρεις.
//! Μόλυνση (other_hz): η Β πολύ χαμηλότερη — 41 κάδοι απόσταση αντί για
//!      3.4.
//! Χρόνος: η Β τάξη μεγέθους γρηγορότερη (ένα FFT 4096 έναντι ~17× FFT
//!      16384).
//! ΤΟ ΑΓΝΩΣΤΟ, ΔΙΑΦΩΝΙΑ ΜΕ ΤΗ ΔΟΘΕΙΣΑ ΠΡΟΒΛΕΨΗ: το local_floor της Β
//!      είναι διάμεσος πάνω σε ~400+ κάδους (±25Hz σε πλέγμα 0.244Hz),
//!      έναντι ~17 κάδων στην Α (±25Hz σε πλέγμα 2.93Hz) — η Β χάνει τη
//!      μέση όρου ΣΤΟΝ ΧΡΟΝΟ (Welch) αλλά κερδίζει πολύ περισσότερη μέση
//!      όρου ΣΤΗ ΣΥΧΝΟΤΗΤΑ (η διάμεσος του πατώματος). Πρόβλεψη: αυτό
//!      μετριάζει τη διασπορά — η Β βρίσκει το p04 με πραγματικό, όχι
//!      οριακό, περιθώριο. Σκεπτικό, όχι μέτρηση· διαψεύσιμο παρακάτω.
//!
//! ΧΡΗΣΗ: cargo run --release --bin hum_detector_methods

use rustfft::{num_complex::Complex, FftPlanner};
use sp314_dsp::dsp::biquad::butter_lp2_prewarped;
use std::time::Instant;

const SR: f32 = 48_000.0;
const NFFT_A: usize = 16_384;
const DECIM: usize = 48;
const DEC_SR: f32 = SR / DECIM as f32; // 1000.0
const NFFT_B: usize = 4_096;
const AA_CUTOFF_HZ: f32 = 450.0;
const AA_SECTIONS: usize = 2; // 4th order total, see ΜΕΤΑ για τον λογαριασμό

/// ΙΔΙΟ με hum_spectrum.rs's dump_mono: decode ΜΙΑ φορά στο dump, run_trunk_pass
/// πάνω στο dump path (όχι στο πρωτότυπο flac — έτσι το καλούσε το
/// hum_spectrum.rs, και είναι το σωστό: το trunk pass θέλει τον ίδιο
/// raw stream που θα δει η ζωντανή αλυσίδα).
fn dump_mono_and_split(path: &str, tag: &str) -> (Vec<f32>, Option<f32>) {
    let dump = format!("/tmp/humdet_{tag}.raw");
    let _ = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .expect("pass0");
    let edge = lineos_types::presets::ACX.room_tone_max_s.unwrap();
    let r = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
        std::path::Path::new(&dump),
        false,
        edge,
    )
    .expect("trunk");
    let split = r.quiet_window_split_dbfs;

    let bytes = std::fs::read(&dump).expect("read dump");
    let n = bytes.len() / 4;
    let mut inter = Vec::with_capacity(n);
    for i in 0..n {
        inter.push(f32::from_le_bytes([
            bytes[i * 4],
            bytes[i * 4 + 1],
            bytes[i * 4 + 2],
            bytes[i * 4 + 3],
        ]));
    }
    let mono: Vec<f32> = inter.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect();
    let _ = std::fs::remove_file(&dump);
    (mono, split)
}

fn rms_db(x: &[f32]) -> f32 {
    let e = x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len() as f64;
    if e < 1e-20 {
        -200.0
    } else {
        (10.0 * e.log10()) as f32
    }
}

/// Ίδιο κριτήριο με hum_spectrum.rs's longest_pause, εφαρμοσμένο σε [0, len).
fn longest_pause(mono: &[f32], thr_db: f32) -> Option<(usize, usize)> {
    let w = 4_800usize; // 100ms @ 48kHz
    let b = mono.len();
    if b <= w {
        return None;
    }
    let (mut best, mut cur_start, mut cur_len) = ((0usize, 0usize), 0usize, 0usize);
    let mut i = 0;
    while i + w <= b {
        if rms_db(&mono[i..i + w]) < thr_db {
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

/// Μέθοδος Α — Welch N=16384, Hann, 50% επικάλυψη. Planner HOISTED (η
/// διόρθωση που ζητήθηκε — ίδιο idiom με fixture_factory.rs's
/// welch_psd_db, stft/mod.rs, analysis/phi1_sensor.rs).
fn welch_psd_db(x: &[f32]) -> (Vec<f32>, usize) {
    let hop = NFFT_A / 2;
    let win: Vec<f32> = (0..NFFT_A)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / NFFT_A as f32).cos())
        .collect();
    let wpow: f64 = win.iter().map(|&v| (v as f64) * (v as f64)).sum();
    let mut acc = vec![0.0f64; NFFT_A / 2 + 1];
    let mut segs = 0usize;
    let mut off = 0usize;
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(NFFT_A);
    while off + NFFT_A <= x.len() {
        let mut buf: Vec<Complex<f32>> = (0..NFFT_A)
            .map(|i| Complex::new(x[off + i] * win[i], 0.0))
            .collect();
        fft.process(&mut buf);
        for (k, a) in acc.iter_mut().enumerate() {
            let p = (buf[k].re as f64).powi(2) + (buf[k].im as f64).powi(2);
            let scale = if k == 0 || k == NFFT_A / 2 { 1.0 } else { 2.0 };
            *a += scale * p / wpow;
        }
        segs += 1;
        off += hop;
    }
    let out = acc
        .iter()
        .map(|&v| {
            let m = v / segs.max(1) as f64;
            if m < 1e-30 {
                -300.0
            } else {
                (10.0 * m.log10()) as f32
            }
        })
        .collect();
    (out, segs)
}

/// Αντι-αναδιπλωτικό: `AA_SECTIONS` × 2ης τάξης Butterworth (prewarped,
/// σωστό Q=1/√2 — ΟΧΙ το mislabeled analysis-path pre_analysis::butter_lp2
/// που το ίδιο του το σχόλιο ομολογεί Q=1.414/resonant, βλ. ΜΕΤΑ).
fn decimate_with_antialias(mono: &[f32]) -> Vec<f32> {
    let mut secs: Vec<_> = (0..AA_SECTIONS)
        .map(|_| butter_lp2_prewarped(AA_CUTOFF_HZ, SR))
        .collect();
    let filtered: Vec<f32> = mono
        .iter()
        .map(|&s| {
            let mut y = s;
            for sec in secs.iter_mut() {
                y = sec.process(y);
            }
            y
        })
        .collect();
    filtered.iter().step_by(DECIM).copied().collect()
}

/// Μέθοδος Β — ΕΝΑ FFT, zero-padded σε NFFT_B. Hann στα πραγματικά
/// δείγματα (μήκος = decimated.len()), μετά padding με μηδενικά.
fn method_b_psd_db(decimated: &[f32]) -> Vec<f32> {
    let n = decimated.len().min(NFFT_B);
    let win: Vec<f32> = (0..n)
        .map(|i| {
            if n <= 1 {
                1.0
            } else {
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos()
            }
        })
        .collect();
    let wpow: f64 = win.iter().map(|&v| (v as f64) * (v as f64)).sum();
    let mut buf: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); NFFT_B];
    for i in 0..n {
        buf[i] = Complex::new(decimated[i] * win[i], 0.0);
    }
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(NFFT_B);
    fft.process(&mut buf);
    (0..=NFFT_B / 2)
        .map(|k| {
            let p = (buf[k].re as f64).powi(2) + (buf[k].im as f64).powi(2);
            let scale = if k == 0 || k == NFFT_B / 2 { 1.0 } else { 2.0 };
            let m = scale * p / wpow;
            if m < 1e-30 {
                -300.0
            } else {
                (10.0 * m.log10()) as f32
            }
        })
        .collect()
}

/// Αυτούσιο από fixture_factory.rs's local_floor_db, γενικευμένο σε
/// bin_hz αντί για σταθερό 48000/16384.
fn local_floor_db(psd: &[f32], k: usize, bin_hz: f32) -> f32 {
    let span = (25.0 / bin_hz) as usize;
    let skip = (4.0 / bin_hz) as usize;
    let lo = k.saturating_sub(span);
    let hi = (k + span).min(psd.len() - 1);
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

fn prominence_at(psd: &[f32], bin_hz: f32, f_hz: f32) -> f32 {
    let k = (f_hz / bin_hz).round() as usize;
    if k >= psd.len() {
        return f32::NAN;
    }
    psd[k] - local_floor_db(psd, k, bin_hz)
}

/// Κορυφή γύρω από target_hz±search_hz, με παραβολική παρεμβολή.
fn peak_near(psd: &[f32], bin_hz: f32, target_hz: f32, search_hz: f32) -> f32 {
    let k_lo = ((target_hz - search_hz) / bin_hz).max(1.0) as usize;
    let k_hi = (((target_hz + search_hz) / bin_hz) as usize).min(psd.len() - 2);
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
    (best_k as f32 + delta) * bin_hz
}

struct FixtureResult {
    name: String,
    hum_hz: Option<f32>,
    other_hz: Option<f32>,
    pause_s: f32,
    a_peak_hz: f32,
    a_prom_own: f32,
    a_prom_other: f32,
    a_ms: [f64; 3],
    b_peak_hz: f32,
    b_prom_own: f32,
    b_prom_other: f32,
    b_ms: [f64; 3],
}

fn main() {
    println!("═══ ΠΡΟΒΛΕΨΗ (βλ. πλήρες κείμενο στο doc-comment της κορυφής του αρχείου) ═══");
    println!("p21,p12: και οι δύο βρίσκουν. p04: η Β καθαρότερα. Μόλυνση: Β πολύ χαμηλότερη.");
    println!("Χρόνος: Β τάξη μεγέθους γρηγορότερη. ΔΙΑΦΩΝΙΑ με τη δοθείσα πρόβλεψη στο ΑΓΝΩΣΤΟ:");
    println!("το local_floor της Β μέσο όρο σε ~400+ κάδους (χωρικά) αντί ~17 (χρονικά, Α) —");
    println!("προβλέπω ότι αυτό μετριάζει τη διασπορά, η Β βρίσκει p04 με πραγματικό περιθώριο.\n");

    let dir = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/tests/fixtures/hum_detector";
    let fixtures: Vec<(&str, Option<f32>, Option<f32>)> = vec![
        ("hum_50hz_p04", Some(50.0), Some(60.0)),
        ("hum_50hz_p12", Some(50.0), Some(60.0)),
        ("hum_50hz_p21", Some(50.0), Some(60.0)),
        ("hum_60hz_p04", Some(60.0), Some(50.0)),
        ("hum_60hz_p12", Some(60.0), Some(50.0)),
        ("hum_60hz_p21", Some(60.0), Some(50.0)),
        ("hum_none", None, None),
    ];

    let mut results = Vec::new();

    for (stem, hum_hz, other_hz) in &fixtures {
        let path = format!("{dir}/{stem}.flac");
        let (mono, split) = dump_mono_and_split(&path, stem);
        let split = split.expect("το αρχείο δεν έδωσε τομή Otsu");
        let (start, len) = longest_pause(&mono, split).expect("καμία παύση");
        let seg = &mono[start..start + len];
        let pause_s = len as f32 / SR;

        // ── Μέθοδος Α, 3 τρεξίματα ──
        let mut a_ms = [0.0; 3];
        let mut psd_a = Vec::new();
        for i in 0..3 {
            let t0 = Instant::now();
            let (p, _segs) = welch_psd_db(seg);
            a_ms[i] = t0.elapsed().as_secs_f64() * 1000.0;
            psd_a = p;
        }
        let bin_hz_a = SR / NFFT_A as f32;
        let (probe_hz_a, other_probe_a) = (hum_hz.unwrap_or(50.0), other_hz.unwrap_or(60.0));
        let a_peak_hz = peak_near(&psd_a, bin_hz_a, probe_hz_a, 10.0);
        let a_prom_own = prominence_at(&psd_a, bin_hz_a, probe_hz_a);
        let a_prom_other = prominence_at(&psd_a, bin_hz_a, other_probe_a);

        // ── Μέθοδος Β, 3 τρεξίματα ──
        let mut b_ms = [0.0; 3];
        let mut psd_b = Vec::new();
        for i in 0..3 {
            let t0 = Instant::now();
            let dec = decimate_with_antialias(seg);
            let p = method_b_psd_db(&dec);
            b_ms[i] = t0.elapsed().as_secs_f64() * 1000.0;
            psd_b = p;
        }
        let bin_hz_b = DEC_SR / NFFT_B as f32;
        let b_peak_hz = peak_near(&psd_b, bin_hz_b, probe_hz_a, 10.0);
        let b_prom_own = prominence_at(&psd_b, bin_hz_b, probe_hz_a);
        let b_prom_other = prominence_at(&psd_b, bin_hz_b, other_probe_a);

        results.push(FixtureResult {
            name: stem.to_string(),
            hum_hz: *hum_hz,
            other_hz: *other_hz,
            pause_s,
            a_peak_hz,
            a_prom_own,
            a_prom_other,
            a_ms,
            b_peak_hz,
            b_prom_own,
            b_prom_other,
            b_ms,
        });
    }

    println!("bin_hz Α = {:.4} Hz (N={NFFT_A}) · bin_hz Β = {:.4} Hz (N={NFFT_B}, decim ×{DECIM} → {DEC_SR}Hz)\n",
        SR / NFFT_A as f32, DEC_SR / NFFT_B as f32);

    println!(
        "{:<14} {:>7} {:>8} {:>9} {:>9} {:>9} | {:>8} {:>9} {:>9} {:>9}",
        "δοκίμιο", "παύση", "Α:peak", "Α:own", "Α:other", "Α:ms×3", "Β:peak", "Β:own", "Β:other", "Β:ms×3"
    );
    for r in &results {
        println!(
            "{:<14} {:>6.2}s {:>7.2}Hz {:>8.2}dB {:>8.2}dB {:>7.2}/{:.2}/{:.2} | {:>7.2}Hz {:>8.2}dB {:>8.2}dB {:>7.2}/{:.2}/{:.2}",
            r.name, r.pause_s, r.a_peak_hz, r.a_prom_own, r.a_prom_other,
            r.a_ms[0], r.a_ms[1], r.a_ms[2],
            r.b_peak_hz, r.b_prom_own, r.b_prom_other,
            r.b_ms[0], r.b_ms[1], r.b_ms[2],
        );
    }

    println!("\n═══ ΤΟ ΑΡΝΗΤΙΚΟ (hum_none) — prominence στα 50 και 60 Hz, ΚΑΙ ΟΙ ΔΥΟ ΜΕΘΟΔΟΙ ═══");
    if let Some(r) = results.iter().find(|r| r.name == "hum_none") {
        println!(
            "  Α: 50Hz {:.2}dB · 60Hz {:.2}dB   (manifest ground truth Α: -0.085 / 0.977 dB)",
            r.a_prom_own, r.a_prom_other
        );
        println!("  Β: 50Hz {:.2}dB · 60Hz {:.2}dB   (κανένα ground truth Β — δεν υπήρχε πριν)",
            r.b_prom_own, r.b_prom_other
        );
    }

    println!("\n═══ ΤΟ p04 ΞΕΧΩΡΙΣΤΑ ═══");
    for r in results.iter().filter(|r| r.name.ends_with("p04")) {
        println!(
            "  {}: Α own={:.2}dB other={:.2}dB (περιθώριο {:.2}dB) | Β own={:.2}dB other={:.2}dB (περιθώριο {:.2}dB)",
            r.name, r.a_prom_own, r.a_prom_other, r.a_prom_own - r.a_prom_other,
            r.b_prom_own, r.b_prom_other, r.b_prom_own - r.b_prom_other
        );
    }
}

