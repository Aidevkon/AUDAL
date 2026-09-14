//! ΜΕΤΡΗΣΗ: ο βόμβος είναι ΔΙΚΤΥΟ ή ΕΥΡΥΖΩΝΙΚΟΣ ΘΟΡΥΒΟΣ;
//!
//! ΖΕΥΓΟΣ ΕΛΕΓΧΟΥ (ακρόαση ιδιοκτήτη, 14/09):
//!   · anne 718–728 s        — βόμβος ΠΑΡΩΝ ΚΑΙ ΣΤΑ ΔΥΟ σκέλη, ΙΣΟΣ
//!   · monte_cristo 95–105 s — OFF βόμβος, ON ανεπαίσθητος
//! Γνωστή απάντηση και στα δύο. Ό,τι ΞΕΧΩΡΙΖΕΙ το ένα από το άλλο είναι
//! ο υποψήφιος.
//!
//! ΟΡΓΑΝΟ: Welch, N=16384, Hann, 50% επικάλυψη ⇒ ανάλυση 2.93 Hz — αρκετή
//! για να ξεχωρίσει 50 από 60 Hz. ΟΧΙ οι οκτώ μπάντες: εκείνες είναι πολύ
//! χοντρές για αυτό το ερώτημα.
//!
//! ΣΗΜΑ: το DUMP της αλυσίδας (48k stereo f32 LE, mono `(l+r)*0.5`) — ό,τι
//! βλέπει ο κόμβος, όχι το mp3.
//!
//! ΠΑΡΑΘΥΡΟ: ΜΟΝΟ ΠΑΥΣΗ. Μέσα στο εύρος που ακούστηκε, ο μεγαλύτερος
//! συνεχόμενος χώρος όπου ΚΑΘΕ παράθυρο 100 ms είναι κάτω από την τομή Otsu
//! του ίδιου του αρχείου (από το trunk pass, ΤΗΝ ΠΑΡΑΓΩΓΗ).
//!
//! ΜΗΔΕΝ κώδικας παραγωγής. ΜΗΔΕΝ απόφαση: τυπώνονται αριθμοί και γράφημα.
//!
//! ΧΡΗΣΗ: cargo run --release --bin hum_spectrum -- <audio> <from_s> <to_s> ...

const NFFT: usize = 16_384;
const SR: f32 = 48_000.0;
const LO_HZ: f32 = 20.0;
const HI_HZ: f32 = 400.0;

fn dump_mono(path: &str, tag: &str) -> (Vec<f32>, Option<f32>) {
    let dump = format!("/tmp/hum_{tag}.raw");
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

/// Ο μεγαλύτερος συνεχόμενος χώρος μέσα στο [from,to] όπου ΚΑΘΕ παράθυρο
/// 100 ms είναι κάτω από το κατώφλι. Επιστρέφει (start_sample, len_samples).
fn longest_pause(mono: &[f32], from_s: f32, to_s: f32, thr_db: f32) -> Option<(usize, usize)> {
    let w = 4_800usize;
    let a = (from_s * SR) as usize;
    let b = ((to_s * SR) as usize).min(mono.len());
    if b <= a + w {
        return None;
    }
    let (mut best, mut cur_start, mut cur_len) = ((0usize, 0usize), a, 0usize);
    let mut i = a;
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

/// Welch PSD σε dB. Hann, 50% επικάλυψη, μέσος όρος ισχύος.
fn welch(x: &[f32]) -> (Vec<f32>, usize) {
    let hop = NFFT / 2;
    let win: Vec<f32> = (0..NFFT)
        .map(|i| {
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / NFFT as f32).cos()
        })
        .collect();
    let wpow: f64 = win.iter().map(|&v| (v as f64) * (v as f64)).sum();
    let mut acc = vec![0.0f64; NFFT / 2 + 1];
    let mut segs = 0usize;
    let mut off = 0usize;
    while off + NFFT <= x.len() {
        // DFT μέσω απλού radix-2: εδώ χρησιμοποιείται το rustfft του δέντρου.
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
        return (vec![-200.0; NFFT / 2 + 1], 0);
    }
    let out = acc
        .iter()
        .map(|&v| {
            let m = v / segs as f64;
            if m < 1e-30 {
                -300.0
            } else {
                (10.0 * m.log10()) as f32
            }
        })
        .collect();
    (out, segs)
}

fn bin_hz(k: usize) -> f32 {
    k as f32 * SR / NFFT as f32
}

/// Τοπικός μέσος γύρω από τον κάδο k: διάμεσος σε ±25 Hz, ΕΞΑΙΡΩΝΤΑΣ ±4 Hz
/// γύρω από την κορυφή ώστε η ίδια η κορυφή να μη σηκώνει τη βάση της.
fn local_floor(psd: &[f32], k: usize) -> f32 {
    let span = (25.0 / (SR / NFFT as f32)) as usize;
    let skip = (4.0 / (SR / NFFT as f32)) as usize;
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

struct Spec {
    stem: String,
    psd: Vec<f32>,
    rows: Vec<(f32, f32)>, // (Hz, dB) ανά ~5 Hz, ΜΕΓΙΣΤΟ των κάδων
    peaks: Vec<(f32, f32, f32)>, // (Hz, dB, dB πάνω από τοπικό μέσο)
    win_s: (f32, f32),
    segs: usize,
    split: Option<f32>,
}

fn analyse(path: &str, from_s: f32, to_s: f32) -> Spec {
    let stem = std::path::Path::new(path)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let (mono, split) = dump_mono(path, &stem);
    // ΠΑΡΑΚΑΜΨΗ ΜΟΝΟ ΓΙΑ ΤΗΝ ΕΠΙΛΟΓΗ ΠΑΡΑΘΥΡΟΥ: αρχείο χωρίς διμερή κατανομή
    // δεν δίνει τομή, άρα δεν υπάρχει κριτήριο παύσης. Το LINEOS_HUM_THR
    // δίνει ένα ΡΗΤΟ dBFS ΜΟΝΟ για το πού θα κοιτάξει το FFT. ΔΕΝ κρίνει
    // τίποτα και ΔΕΝ γράφεται πουθενά — αν χρησιμοποιηθεί, φαίνεται στην
    // επικεφαλίδα της γραμμής.
    let thr = match std::env::var("LINEOS_HUM_THR").ok().and_then(|v| v.parse::<f32>().ok()) {
        Some(v) => v,
        None => split.expect("το αρχείο δεν έδωσε τομή Otsu — δεν υπάρχει κριτήριο παύσης"),
    };
    let (start, len) =
        longest_pause(&mono, from_s, to_s, thr).expect("καμία παύση κάτω από την τομή στο εύρος");
    let seg = &mono[start..start + len];
    let (psd, segs) = welch(seg);

    // ── γραμμές ~5 Hz: ΜΕΓΙΣΤΟ των κάδων, ώστε μια στενή γραμμή να επιβιώνει ──
    let k_lo = (LO_HZ / (SR / NFFT as f32)) as usize;
    let k_hi = (HI_HZ / (SR / NFFT as f32)) as usize;
    let per_row = (5.0 / (SR / NFFT as f32)).round().max(1.0) as usize;
    let mut rows = Vec::new();
    let mut k = k_lo;
    while k <= k_hi {
        let e = (k + per_row).min(k_hi + 1);
        let m = (k..e).map(|i| psd[i]).fold(f32::NEG_INFINITY, f32::max);
        rows.push((bin_hz(k), m));
        k = e;
    }

    // ── κορυφές σε ΠΛΗΡΗ ανάλυση ──
    let mut cand: Vec<(f32, f32, f32)> = Vec::new();
    for i in (k_lo + 1)..k_hi {
        if psd[i] > psd[i - 1] && psd[i] >= psd[i + 1] {
            let f = local_floor(&psd, i);
            cand.push((bin_hz(i), psd[i], psd[i] - f));
        }
    }
    cand.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    // Κρατάμε τις πέντε πιο ξεχωριστές, χωρίς δύο μέσα σε 8 Hz.
    let mut peaks: Vec<(f32, f32, f32)> = Vec::new();
    for c in cand {
        if peaks.iter().all(|p| (p.0 - c.0).abs() > 8.0) {
            peaks.push(c);
        }
        if peaks.len() == 5 {
            break;
        }
    }

    Spec {
        stem,
        psd,
        rows,
        peaks,
        win_s: (start as f32 / SR, (start + len) as f32 / SR),
        segs,
        split,
    }
}

fn harmonic_note(f: f32) -> String {
    let n50 = (f / 50.0).round().max(1.0);
    let n60 = (f / 60.0).round().max(1.0);
    let d50 = (f - n50 * 50.0).abs();
    let d60 = (f - n60 * 60.0).abs();
    format!(
        "50x{:<2.0} {:+5.1} Hz | 60x{:<2.0} {:+5.1} Hz",
        n50,
        f - n50 * 50.0,
        n60,
        f - n60 * 60.0
    ) + if d50 < d60 { "  <-50" } else { "  <-60" }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        a.len() >= 3 && a.len() % 3 == 0,
        "usage: hum_spectrum <audio> <from_s> <to_s> [...]"
    );
    let mut specs = Vec::new();
    for c in a.chunks(3) {
        specs.push(analyse(
            &c[0],
            c[1].parse().expect("from_s"),
            c[2].parse().expect("to_s"),
        ));
    }

    println!("Welch N={NFFT} · Hann · 50% επικάλυψη · ανάλυση {:.2} Hz", SR / NFFT as f32);
    println!("παράθυρο: ΜΟΝΟ παύση (κάθε 100 ms κάτω από την τομή Otsu του αρχείου)");
    println!("γραμμές ανά ~5 Hz, τιμή = ΜΕΓΙΣΤΟ των κάδων (μια στενή γραμμή επιβιώνει)\n");

    for s in &specs {
        println!(
            "  {:<34} παύση {:.2}–{:.2} s ({:.2} s) · τμήματα Welch {} · τομή {:?}",
            s.stem,
            s.win_s.0,
            s.win_s.1,
            s.win_s.1 - s.win_s.0,
            s.segs,
            s.split
        );
    }
    println!();

    // ── ΔΙΠΛΑ-ΔΙΠΛΑ ──
    let hdrs: Vec<&str> = specs.iter().map(|s| s.stem.as_str()).collect();
    print!("{:>7}", "Hz");
    for h in &hdrs {
        print!("  {:<34}", &h[..h.len().min(34)]);
    }
    println!();
    // ΚΟΙΝΗ ΚΛΙΜΑΚΑ: τα δύο γραφήματα συγκρίνονται μόνο αν μοιράζονται άξονα.
    let gmax = specs
        .iter()
        .flat_map(|s| s.rows.iter().map(|r| r.1))
        .fold(f32::NEG_INFINITY, f32::max);
    let gmin = gmax - 60.0;
    println!(
        "        κοινή κλίμακα {:.1} .. {:.1} dB, 34 στήλες = 60 dB\n",
        gmin, gmax
    );
    let n = specs[0].rows.len();
    for i in 0..n {
        print!("{:>7.0}", specs[0].rows[i].0);
        for s in &specs {
            let v = s.rows[i].1;
            let w = (((v - gmin) / 60.0).clamp(0.0, 1.0) * 34.0).round() as usize;
            print!("  {:<34}", "#".repeat(w));
        }
        println!("   {:>7.1}{}", specs[0].rows[i].1, {
            let mut t = String::new();
            for s in specs.iter().skip(1) {
                t.push_str(&format!(" {:>7.1}", s.rows[i].1));
            }
            t
        });
    }

    // ── ΩΜΟΙ ΚΑΔΟΙ ΓΥΡΩ ΑΠΟ 50 ΚΑΙ 60 Hz ──
    // Η ανάλυση είναι 2.93 Hz· ο «58.59» είναι ΚΑΔΟΣ, όχι εκτίμηση συχνότητας.
    // Ένας τόνος 60 Hz χύνεται σε δύο γειτονικούς κάδους. Τυπώνονται ωμοί,
    // και δίπλα παραβολική παρεμβολή της κορυφής — το πρότυπο μέτρο ακρίβειας
    // για κορυφή σε FFT με παράθυρο Hann.
    println!("\n═══ ΩΜΟΙ ΚΑΔΟΙ 40–75 Hz (ανάλυση {:.2} Hz) ═══", SR / NFFT as f32);
    print!("{:>9}", "Hz");
    for s in &specs { print!("{:>12}", &s.stem[..s.stem.len().min(11)]); }
    println!();
    let k0 = (40.0 / (SR / NFFT as f32)) as usize;
    let k1 = (75.0 / (SR / NFFT as f32)) as usize;
    for k in k0..=k1 {
        print!("{:>9.2}", bin_hz(k));
        for s in &specs { print!("{:>12.1}", s.psd[k]); }
        println!();
    }
    println!();
    for s in &specs {
        // κορυφή στο 40–75 Hz, με παραβολική παρεμβολή σε dB
        let mut kp = k0;
        for k in k0..=k1 { if s.psd[k] > s.psd[kp] { kp = k; } }
        let (a, b, c) = (s.psd[kp - 1], s.psd[kp], s.psd[kp + 1]);
        let d = 0.5 * (a - c) / (a - 2.0 * b + c);
        let fhat = bin_hz(kp) + d * (SR / NFFT as f32);
        println!(
            "  {:<34} κορυφή κάδος {:.2} Hz · ΠΑΡΕΜΒΟΛΗ {:.2} Hz  (απόσταση: 50 {:+.2} · 60 {:+.2})",
            s.stem, bin_hz(kp), fhat, fhat - 50.0, fhat - 60.0
        );
    }

    println!("\n═══ ΟΙ ΠΕΝΤΕ ΜΕΓΑΛΥΤΕΡΕΣ ΚΟΡΥΦΕΣ ΑΝΑ ΑΡΧΕΙΟ ═══");
    println!("«πάνω από τοπικό μέσο» = διάμεσος σε ±25 Hz, εξαιρώντας ±4 Hz γύρω από την κορυφή\n");
    for s in &specs {
        println!("── {} ──", s.stem);
        println!(
            "  {:>9}{:>10}{:>12}   {}",
            "Hz", "dB", "πάνω dB", "απόσταση από αρμονική"
        );
        for (f, d, a) in &s.peaks {
            println!("  {f:>9.2}{d:>10.1}{a:>12.2}   {}", harmonic_note(*f));
        }
        // Οι ίδιες οι αρμονικές, ονομαστικά — ό,τι κι αν βγήκε στις κορυφές.
        println!("  ΟΝΟΜΑΣΤΙΚΕΣ ΑΡΜΟΝΙΚΕΣ (τιμή στον πλησιέστερο κάδο, και πάνω από τοπικό μέσο):");
        for f in [50.0, 60.0, 100.0, 120.0, 150.0, 180.0, 200.0, 240.0] {
            let k = (f / (SR / NFFT as f32)).round() as usize;
            let fl = local_floor(&s.psd, k);
            println!(
                "     {f:>6.0} Hz  {:>8.1} dB   {:+7.2} dB πάνω",
                s.psd[k],
                s.psd[k] - fl
            );
        }
        println!();
    }
}
