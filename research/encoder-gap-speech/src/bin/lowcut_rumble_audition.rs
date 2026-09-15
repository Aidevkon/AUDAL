//! ΜΕΤΡΗΣΗ: τι κάνει ο lowcut (cleaner.rs:69, HighPass 80Hz Q=0.707) σε
//! φωνή ΜΕ rumble. Συνέχεια της ΜΕΤΡΗΣΗ R-01 (FINDINGS.md, 2026-09-01),
//! που μέτρησε τον ΙΔΙΟ φίλτρο σε καθαρή φωνή (B1 Δ=-0.158dB) και δήλωσε
//! ρητά «Εκκρεμεί: ίδια μέτρηση σε υλικό με πλούσιο rumble».
//!
//! ΙΔΙΟ ΣΧΗΜΑ ΜΕ expander_audition.rs: OFF/ON ζεύγη, ΜΟΝΟ ο κόμβος υπό
//! εξέταση αλλάζει. Εδώ ο κόμβος καλείται ΑΠΕΥΘΕΙΑΣ (RestorationChain
//! με lowcut_enabled μόνο) αντί μέσω της πλήρους streaming διαδρομής —
//! το lowcut είναι per-sample φίλτρο, δεν χρειάζεται stems/segmentation
//! για να απομονωθεί.
//!
//! welch()/local_floor_of()/bin_hz(): ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ από
//! hum_spectrum.rs (ίδιες παράμετροι: Welch N=16384, Hann, 50%
//! επικάλυψη, LO_HZ=20/HI_HZ=400) — ώστε η "πτώση κορυφής" να μετριέται
//! με το ΙΔΙΟ όργανο πριν/μετά, όχι νέο.
//!
//! ΜΗΔΕΝ αλλαγή παραγωγής. ΜΗΔΕΝ κατώφλι.

use sp314_dsp::restoration::{RestorationChain, RestorationConfig};

const NFFT: usize = 16_384;
const SR: f32 = 48_000.0;
const LO_HZ: f32 = 20.0;
const HI_HZ: f32 = 400.0;

// ═══ Η ΠΡΟΒΛΕΨΗ, ΓΡΑΜΜΕΝΗ ΠΡΙΝ ΤΡΕΞΕΙ ΟΤΙΔΗΠΟΤΕ ═══
// Η κορυφή στα ~26Hz πέφτει ~20dB (η εξασθένηση του φίλτρου εκεί, μετρημένη
// 15/09 από τον ίδιο τον ψηφιακό μετασχηματισμό). Το B1 (80-250Hz) μένει
// κάτω από 0.5dB ακόμα και με rumble, γιατί το φίλτρο δίνει μόλις -1.5dB
// στα 100Hz. Το ΔLUFS ~0.
// ΑΓΝΩΣΤΟ: αν το rumble είναι τόσο δυνατό που συνεισφέρει στο LUFS, το
// ΔLUFS ΔΕΝ θα είναι μηδέν — και τότε το φίλτρο αλλάζει στάθμη, όχι μόνο
// φάσμα.

struct FileResult {
    name: String,
    off_spectral: [f32; 8],
    on_spectral: [f32; 8],
    off_lufs: f32,
    on_lufs: f32,
    off_interior: Option<f32>,
    on_interior: Option<f32>,
    pause_peak_hz: Option<f32>,
    off_peak_db: Option<f32>,
    on_peak_db: Option<f32>,
    prominence_ref: f32,
    speech_window: Option<(f32, f32)>, // (start_s, dur_s)
    speech_fund_hz: Option<f32>,
    speech_off_db: Option<f32>,
    speech_on_db: Option<f32>,
    speech_expected_drop_db: Option<f32>,
}

fn decode_dump(path: &str, tag: &str) -> String {
    let dump = format!("/tmp/lowcut_rumble_{tag}.raw");
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .expect("pass0 decode");
    dump
}

fn read_dump_stereo(dump: &str) -> (Vec<f32>, Vec<f32>) {
    let bytes = std::fs::read(dump).expect("read dump");
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
    let l: Vec<f32> = inter.chunks_exact(2).map(|f| f[0]).collect();
    let r: Vec<f32> = inter.chunks_exact(2).map(|f| f[1]).collect();
    (l, r)
}

fn write_dump_stereo(dump: &str, l: &[f32], r: &[f32]) {
    let mut buf = Vec::with_capacity(l.len() * 8);
    for i in 0..l.len() {
        buf.extend_from_slice(&l[i].to_le_bytes());
        buf.extend_from_slice(&r[i].to_le_bytes());
    }
    std::fs::write(dump, buf).expect("write dump");
}

fn lufs_of(l: &[f32], r: &[f32]) -> f32 {
    let mut m = sp314_dsp::metering::LufsMeter::new();
    const CHUNK: usize = 4096;
    let mut i = 0;
    while i < l.len() {
        let e = (i + CHUNK).min(l.len());
        m.process_chunk(&l[i..e], &r[i..e]);
        i = e;
    }
    m.finish().unwrap_or(-144.0)
}

// ── ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ από hum_spectrum.rs (welch/local_floor/bin_hz) ──
fn welch(x: &[f32]) -> Vec<f32> {
    let hop = NFFT / 2;
    let win: Vec<f32> = (0..NFFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / NFFT as f32).cos())
        .collect();
    let wpow: f64 = win.iter().map(|&v| (v as f64) * (v as f64)).sum();
    let mut acc = vec![0.0f64; NFFT / 2 + 1];
    let mut segs = 0usize;
    let mut off = 0usize;
    while off + NFFT <= x.len() {
        let mut buf: Vec<rustfft::num_complex::Complex<f32>> = (0..NFFT)
            .map(|i| rustfft::num_complex::Complex::new(x[off + i] * win[i], 0.0))
            .collect();
        let mut planner = rustfft::FftPlanner::new();
        let fft = planner.plan_fft_forward(NFFT);
        fft.process(&mut buf);
        for (k, a) in acc.iter_mut().enumerate() {
            let p = (buf[k].re as f64).powi(2) + (buf[k].im as f64).powi(2);
            let scale = if k == 0 || k == NFFT / 2 { 1.0 } else { 2.0 };
            *a += scale * p / wpow;
        }
        segs += 1;
        off += hop;
    }
    if segs == 0 {
        return vec![-200.0; NFFT / 2 + 1];
    }
    acc.iter()
        .map(|&v| {
            let m = v / segs as f64;
            if m < 1e-30 {
                -300.0
            } else {
                (10.0 * m.log10()) as f32
            }
        })
        .collect()
}

fn bin_hz(k: usize) -> f32 {
    k as f32 * SR / NFFT as f32
}

fn hz_to_bin(hz: f32) -> usize {
    (hz / (SR / NFFT as f32)).round() as usize
}

fn peak_db_near(psd: &[f32], target_hz: f32, tol_hz: f32) -> f32 {
    let lo = hz_to_bin((target_hz - tol_hz).max(LO_HZ));
    let hi = hz_to_bin((target_hz + tol_hz).min(HI_HZ)).min(psd.len() - 1);
    (lo..=hi).map(|i| psd[i]).fold(f32::NEG_INFINITY, f32::max)
}

/// Η αναλυτική απόκριση του ΙΔΙΟΥ ψηφιακού φίλτρου (cleaner.rs:69:
/// HighPass 80Hz Q=0.707 @48kHz) σε οποιαδήποτε συχνότητα — bilinear,
/// όχι ιδανικό αναλογικό, ίδιος τύπος με τον υπολογισμό της 15/09
/// (25·50·80·100·150Hz). Χρησιμοποιείται ΜΟΝΟ για να προβλεφθεί η πτώση
/// στη θεμελιώδη ΠΡΙΝ συγκριθεί με το μετρημένο — δεν αγγίζει το φίλτρο.
fn filter_response_db(freq_hz: f32) -> f32 {
    let omega = 2.0 * std::f32::consts::PI * 80.0 / SR;
    let q = 0.707_f32;
    let alpha = omega.sin() / (2.0 * q);
    let cos_w = omega.cos();
    let b0 = (1.0 + cos_w) / 2.0;
    let b1 = -(1.0 + cos_w);
    let b2 = (1.0 + cos_w) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - alpha;
    let (b0, b1, b2, a1, a2) = (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);

    let w = 2.0 * std::f32::consts::PI * freq_hz / SR;
    let (cw, sw) = (w.cos(), w.sin());
    // z^-1 = cos(w) - i sin(w); z^-2 = cos(2w) - i sin(2w)
    let (c2, s2) = ((2.0 * w).cos(), (2.0 * w).sin());
    let num_re = b0 + b1 * cw + b2 * c2;
    let num_im = -(b1 * sw + b2 * s2);
    let den_re = 1.0 + a1 * cw + a2 * c2;
    let den_im = -(a1 * sw + a2 * s2);
    let num_mag = (num_re * num_re + num_im * num_im).sqrt();
    let den_mag = (den_re * den_re + den_im * den_im).sqrt();
    20.0 * (num_mag / den_mag).log10()
}

/// ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ, ανεστραμμένο κριτήριο από
/// `sp314_dsp::analysis::mains_hum::longest_run_below` (ίδιο παράθυρο
/// 100ms, ίδιος τύπος RMS-σε-dB με `pause_window_rms_db` — mains_hum.rs
/// είναι ιδιωτική εκεί). Βρίσκει τη ΜΕΓΑΛΥΤΕΡΗ συνεχόμενη περιοχή όπου
/// ΚΑΘΕ παράθυρο 100ms είναι >= thr_db — το αντίθετο κριτήριο από την
/// παύση, με το ΙΔΙΟ όργανο (ίδιο thr_db: quiet_window_split_dbfs).
fn longest_run_above(samples: &[f32], sample_rate: u32, thr_db: f32) -> Option<(usize, usize)> {
    let w = (sample_rate as f32 * 100.0 / 1000.0) as usize;
    if w == 0 || samples.len() < w {
        return None;
    }
    let win_db = |chunk: &[f32]| -> f32 {
        let e = chunk.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / chunk.len() as f64;
        if e < 1e-20 { -200.0 } else { (10.0 * e.log10()) as f32 }
    };
    let (mut best, mut cur_start, mut cur_len) = ((0usize, 0usize), 0usize, 0usize);
    let mut i = 0usize;
    while i + w <= samples.len() {
        if win_db(&samples[i..i + w]) >= thr_db {
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

fn extract_speech_window(mono: &[f32], sr: f32, dur_s: f32) -> f32 {
    // Πιο δυνατό συνεχόμενο παράθυρο dur_s δευτερολέπτων, κατά μέση RMS —
    // απλό κριτήριο "εδώ σίγουρα μιλάει κάποιος", όχι VAD.
    let win = (dur_s * sr) as usize;
    if mono.len() < win {
        return 0.0;
    }
    let step = (1.0 * sr) as usize; // σάρωση ανά 1s
    let mut best_start = 0usize;
    let mut best_rms = f32::NEG_INFINITY;
    let mut i = 0usize;
    while i + win <= mono.len() {
        let seg = &mono[i..i + win];
        let ms: f32 = seg.iter().map(|&s| s * s).sum::<f32>() / seg.len() as f32;
        let rms = ms.sqrt();
        if rms > best_rms {
            best_rms = rms;
            best_start = i;
        }
        i += step;
    }
    best_start as f32 / sr
}

fn measure_file(path: &str, name: &str, prominence_ref: f32) -> FileResult {
    let off_dump = decode_dump(path, &format!("{name}_off"));
    let off_report =
        sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
            std::path::Path::new(&off_dump), false, 5.0,
        )
        .expect("trunk off");

    let (l0, r0) = read_dump_stereo(&off_dump);

    // ON: ΜΟΝΟ lowcut_enabled=true, όλα τα άλλα false.
    let mut chain_on = RestorationChain::new(
        SR,
        RestorationConfig {
            lowcut_enabled: true,
            hum_enabled: false,
            deess_enabled: false,
            gate_enabled: false,
        },
        0.0,
        f32::NEG_INFINITY,
    );
    let mut l1 = l0.clone();
    let mut r1 = r0.clone();
    chain_on.process(&mut l1, &mut r1);

    let on_dump = format!("/tmp/lowcut_rumble_{name}_on.raw");
    write_dump_stereo(&on_dump, &l1, &r1);
    let on_report =
        sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
            std::path::Path::new(&on_dump), false, 5.0,
        )
        .expect("trunk on");

    let off_lufs = lufs_of(&l0, &r0);
    let on_lufs = lufs_of(&l1, &r1);

    // ── Υψηλής ανάλυσης φάσμα στην ΙΔΙΑ παύση, OFF έναντι ON ──
    let mono_off: Vec<f32> = l0.iter().zip(&r0).map(|(a, b)| (a + b) * 0.5).collect();
    let mono_on: Vec<f32> = l1.iter().zip(&r1).map(|(a, b)| (a + b) * 0.5).collect();
    let (pause_peak_hz, off_peak_db, on_peak_db) = match off_report.quiet_window_split_dbfs {
        Some(thr) => {
            match sp314_dsp::analysis::mains_hum::longest_run_below(&mono_off, SR as u32, thr, None) {
                Some((start, len)) if len >= NFFT => {
                    let seg_off = &mono_off[start..start + len];
                    let seg_on = &mono_on[start..start + len];
                    let psd_off = welch(seg_off);
                    let psd_on = welch(seg_on);
                    // Κορυφή OFF στη ζώνη [LO_HZ,HI_HZ]: το ΜΕΓΙΣΤΟ bin.
                    let k_lo = hz_to_bin(LO_HZ);
                    let k_hi = hz_to_bin(HI_HZ).min(psd_off.len() - 1);
                    let (peak_k, _) = (k_lo..=k_hi)
                        .map(|k| (k, psd_off[k]))
                        .fold((k_lo, f32::NEG_INFINITY), |acc, x| if x.1 > acc.1 { x } else { acc });
                    let peak_hz = bin_hz(peak_k);
                    let off_db = peak_db_near(&psd_off, peak_hz, 3.0);
                    let on_db = peak_db_near(&psd_on, peak_hz, 3.0);
                    (Some(peak_hz), Some(off_db), Some(on_db))
                }
                _ => (None, None, None),
            }
        }
        None => (None, None, None),
    };

    // ── Σημείο Β: μέσα σε ομιλία, το αντίθετο κριτήριο από την παύση ──
    // Ελάχιστο 2.0s ζητούμενο. Αν το μεγαλύτερο συνεχές τμήμα πάνω από
    // την τομή είναι μικρότερο, ΑΠΟΝ — δηλωμένο, όχι μικρότερο παράθυρο
    // σιωπηλά.
    const MIN_SPEECH_S: f32 = 2.0;
    const TARGET_SPEECH_S: f32 = 3.0;
    let (speech_window, speech_fund_hz, speech_off_db, speech_on_db, speech_expected_drop_db) =
        match off_report.quiet_window_split_dbfs {
            Some(thr) => match longest_run_above(&mono_off, SR as u32, thr) {
                Some((start, len)) if (len as f32 / SR) >= MIN_SPEECH_S => {
                    let use_len = (len as usize).min((TARGET_SPEECH_S * SR) as usize);
                    let seg_off = &mono_off[start..start + use_len];
                    let seg_on = &mono_on[start..start + use_len];
                    if use_len >= NFFT {
                        let psd_off = welch(seg_off);
                        let psd_on = welch(seg_on);
                        // Θεμελιώδης: μέγιστο bin OFF στη ζώνη 70-250Hz.
                        let k_lo = hz_to_bin(70.0);
                        let k_hi = hz_to_bin(250.0).min(psd_off.len() - 1);
                        let (peak_k, _) = (k_lo..=k_hi)
                            .map(|k| (k, psd_off[k]))
                            .fold((k_lo, f32::NEG_INFINITY), |acc, x| if x.1 > acc.1 { x } else { acc });
                        let fund_hz = bin_hz(peak_k);
                        let off_db = peak_db_near(&psd_off, fund_hz, 3.0);
                        let on_db = peak_db_near(&psd_on, fund_hz, 3.0);
                        let expected = filter_response_db(fund_hz);
                        (
                            Some((start as f32 / SR, use_len as f32 / SR)),
                            Some(fund_hz),
                            Some(off_db),
                            Some(on_db),
                            Some(expected),
                        )
                    } else {
                        (Some((start as f32 / SR, use_len as f32 / SR)), None, None, None, None)
                    }
                }
                Some((_, len)) => {
                    println!(
                        "  ΣΗΜΕΙΟ Β ΑΠΟΝ: μεγαλύτερο συνεχές τμήμα ομιλίας {:.2}s < {MIN_SPEECH_S}s ζητούμενο",
                        len as f32 / SR
                    );
                    (None, None, None, None, None)
                }
                None => {
                    println!("  ΣΗΜΕΙΟ Β ΑΠΟΝ: καμία συνεχής περιοχή πάνω από την τομή Otsu");
                    (None, None, None, None, None)
                }
            },
            None => {
                println!("  ΣΗΜΕΙΟ Β ΑΠΟΝ: καμία τομή Otsu για αυτό το αρχείο");
                (None, None, None, None, None)
            }
        };

    let _ = std::fs::remove_file(&off_dump);
    let _ = std::fs::remove_file(&on_dump);

    FileResult {
        name: name.to_string(),
        off_spectral: off_report.spectral_profile_db,
        on_spectral: on_report.spectral_profile_db,
        off_lufs,
        on_lufs,
        off_interior: off_report.acx_interior_noise_floor.map(|(db, _)| db),
        on_interior: on_report.acx_interior_noise_floor.map(|(db, _)| db),
        pause_peak_hz,
        off_peak_db,
        on_peak_db,
        prominence_ref,
        speech_window,
        speech_fund_hz,
        speech_off_db,
        speech_on_db,
        speech_expected_drop_db,
    }
}

fn main() {
    println!("{}", include_str!("lowcut_rumble_prediction.txt"));

    println!("═══ ΑΥΤΟ-ΕΛΕΓΧΟΣ: απόκριση φίλτρου (πρέπει ≈ 25:-20.25 · 50:-8.78 · 80:-3.01 · 100:-1.49 · 150:-0.34 dB, μετρημένα 15/09) ═══");
    for hz in [25.0_f32, 50.0, 80.0, 100.0, 150.0] {
        println!("  {:.0} Hz: {:.2} dB", hz, filter_response_db(hz));
    }
    println!();

    let base = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    // (όνομα, προεξοχή αναφοράς από hum-detector-recon-20260914.txt — "ΜΕ" ταξινομημένα φθίνοντα, μετά "ΧΩΡΙΣ")
    let files: [(&str, f32); 9] = [
        ("secretgarden_01_burnett", 20.96),
        ("dracula_01_stoker", 19.77),
        ("janeeyre_01_bronte", 17.90),
        ("huckfinn_01_twain_apc", 4.48),
        ("peterpan_01_barrie", 3.52),
        ("tale_of_two_cities_01_dickens", 3.49),
        ("adventurespinocchio_01_collodi", f32::NAN),
        ("anne_of_green_gables_01_montgomery", f32::NAN),
        ("count_of_monte_cristo_001_dumas", f32::NAN),
    ];

    let mut results = Vec::new();
    for (name, prom) in files {
        let path = format!("{base}/{name}.mp3");
        println!("--- {name} ---");
        let r = measure_file(&path, name, prom);
        println!(
            "  B0 OFF={:.3} ON={:.3} Δ={:.3} | B1 OFF={:.3} ON={:.3} Δ={:.3}",
            r.off_spectral[0], r.on_spectral[0], r.on_spectral[0] - r.off_spectral[0],
            r.off_spectral[1], r.on_spectral[1], r.on_spectral[1] - r.off_spectral[1],
        );
        println!(
            "  LUFS OFF={:.4} ON={:.4} Δ={:.4}",
            r.off_lufs, r.on_lufs, r.on_lufs - r.off_lufs
        );
        println!(
            "  interior OFF={:?} ON={:?}",
            r.off_interior, r.on_interior
        );
        match (r.pause_peak_hz, r.off_peak_db, r.on_peak_db) {
            (Some(hz), Some(off), Some(on)) => println!(
                "  ΣΗΜΕΙΟ Α (παύση) κορυφή {:.2}Hz: OFF={:.2}dB ON={:.2}dB Δ={:.2}dB",
                hz, off, on, on - off
            ),
            _ => println!("  ΣΗΜΕΙΟ Α (παύση): καμία κορυφή/παύση μετρήθηκε"),
        }
        if let (Some((ws, wd)), Some(hz), Some(off), Some(on), Some(exp)) = (
            r.speech_window, r.speech_fund_hz, r.speech_off_db, r.speech_on_db, r.speech_expected_drop_db,
        ) {
            let measured_drop = on - off;
            let agree = (measured_drop - exp).abs() <= 1.0;
            println!(
                "  ΣΗΜΕΙΟ Β (ομιλία @{:.1}s, {:.2}s) θεμελιώδης {:.2}Hz: OFF={:.2}dB ON={:.2}dB Δ={:.2}dB · αναμενόμενη {:.2}dB · {}",
                ws, wd, hz, off, on, measured_drop, exp,
                if agree { "ΣΥΜΦΩΝΟΥΝ (±1dB)" } else { "⚠ ΔΙΑΦΩΝΟΥΝ" }
            );
        }
        results.push(r);
    }

    println!("\n═══ ΣΥΝΟΨΗ ═══");
    println!("{:<38} {:>8} {:>9} {:>9} {:>9} {:>9}", "αρχείο", "prom_ref", "ΔB0", "ΔB1", "ΔLUFS", "Δpeak");
    for r in &results {
        println!(
            "{:<38} {:>8.2} {:>9.3} {:>9.3} {:>9.4} {:>9}",
            r.name,
            r.prominence_ref,
            r.on_spectral[0] - r.off_spectral[0],
            r.on_spectral[1] - r.off_spectral[1],
            r.on_lufs - r.off_lufs,
            r.off_peak_db.zip(r.on_peak_db).map(|(o, n)| format!("{:.2}", n - o)).unwrap_or_else(|| "ΑΠΟΝ".into()),
        );
    }

    // ── Αποσπάσματα ακρόασης: τα τρία με τη μεγαλύτερη προεξοχή, OFF/ON, ΜΕ ΟΜΙΛΙΑ ──
    println!("\n═══ ΑΠΟΣΠΑΣΜΑΤΑ (τα τρία με τη μεγαλύτερη προεξοχή) ═══");
    let out_dir = "/tmp/lowcut-rumble-audition";
    let _ = std::fs::create_dir_all(out_dir);
    let mut top3: Vec<&FileResult> = results.iter().filter(|r| !r.prominence_ref.is_nan()).collect();
    top3.sort_by(|a, b| b.prominence_ref.partial_cmp(&a.prominence_ref).unwrap());
    top3.truncate(3);

    for r in &top3 {
        let path = format!("{base}/{}.mp3", r.name);
        let off_dump = decode_dump(&path, &format!("{}_clip", r.name));
        let (l0, r0) = read_dump_stereo(&off_dump);
        let mono: Vec<f32> = l0.iter().zip(&r0).map(|(a, b)| (a + b) * 0.5).collect();
        let speech_start_s = extract_speech_window(&mono, SR, 10.0);

        let mut chain_on = RestorationChain::new(
            SR,
            RestorationConfig { lowcut_enabled: true, hum_enabled: false, deess_enabled: false, gate_enabled: false },
            0.0,
            f32::NEG_INFINITY,
        );
        let mut l1 = l0.clone();
        let mut r1 = r0.clone();
        chain_on.process(&mut l1, &mut r1);

        let start_frame = (speech_start_s * SR) as usize;
        let len_frame = (10.0 * SR) as usize;
        let end_frame = (start_frame + len_frame).min(l0.len());

        let off_path = format!("{out_dir}/{}_OFF_prom{:.2}dB_speech@{:.1}s.wav", r.name, r.prominence_ref, speech_start_s);
        let on_path = format!("{out_dir}/{}_ON_prom{:.2}dB_speech@{:.1}s.wav", r.name, r.prominence_ref, speech_start_s);
        write_wav(&off_path, &l0[start_frame..end_frame], &r0[start_frame..end_frame]);
        write_wav(&on_path, &l1[start_frame..end_frame], &r1[start_frame..end_frame]);
        println!("  {off_path}");
        println!("  {on_path}");

        let _ = std::fs::remove_file(&off_dump);
    }
}

fn write_wav(path: &str, l: &[f32], r: &[f32]) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("create wav");
    for i in 0..l.len() {
        w.write_sample(l[i]).unwrap();
        w.write_sample(r[i]).unwrap();
    }
    w.finalize().unwrap();
}
