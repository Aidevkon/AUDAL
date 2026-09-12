//! Η γεννήτρια του πίνακα αλληλεπίδρασης των
//! οκτώ LTASS φίλτρων, μετρημένου στις
//! ΟΛΟΚΛΗΡΩΜΕΝΕΣ μπάντες — όχι στις κεντρικές
//! συχνότητες (F-098: οι δύο διαφέρουν κατά 40×).
//!
//! ΔΗΛΩΜΕΝΟ ΟΡΙΟ: το σύστημα ΔΕΝ είναι αυστηρά
//! γραμμικό. Ο λόγος παράδοσης της μπάντας 0
//! μετακινείται με το μέγεθος του ζητούμενου:
//! +3 → 0.7193 · +6 → 0.7340 · +12 → 0.7539,
//! δηλαδή ~5% σε εύρος 4×. Ο πίνακας είναι
//! ΠΡΟΣΕΓΓΙΣΗ ΠΡΩΤΗΣ ΤΑΞΗΣ γύρω από το +6 dB.
//! Μακριά από εκεί, το υπόλοιπο σφάλμα μεγαλώνει.
//!
//! ΚΑΙ ΤΟ Q: ο πίνακας ισχύει ΜΟΝΟ για το Q με το
//! οποίο μετρήθηκε. Άλλο Q ⇒ ξανατρέξ' το.
//! (Το F-093 κατέγραψε 64 συντελεστές από
//!  γεννήτρια που δεν υπήρξε ποτέ στο repo· αυτό
//!  το αρχείο υπάρχει γι' αυτόν τον λόγο.)
//!
//! ⚠ ΤΙ ΔΕΝ ΚΑΤΕΓΡΑΨΕ ΤΟ F-098, ΚΑΙ ΞΑΝΑΒΡΕΘΗΚΕ ΕΔΩ
//! ---------------------------------------------------------------
//! Το F-098 κατέγραψε «ντετερμινιστικός LCG seed 0x5EED1234» αλλά ΟΧΙ
//! τη μετατροπή κατάστασης→δείγματος. Στο δέντρο υπάρχει ΕΝΑΣ LCG
//! (1664525 / 1013904223, σε 8+ σημεία) αλλά ΤΡΕΙΣ διαφορετικές
//! μετατροπές. Το `scratchpad/bmat/` που το F-098 δηλώνει ως τεκμήριο
//! ΔΕΝ ΥΠΑΡΧΕΙ ΠΙΑ.
//! ⇒ Η μετατροπή ΔΕΝ κρύβεται σε μία επιλογή: είναι όρισμα γραμμής
//!   εντολών (`noise_map` 0|1|2), ώστε ο επόμενος να μπορεί να
//!   ΜΕΤΡΗΣΕΙ αν έχει σημασία αντί να το υποθέσει. Οι τρεις είναι
//!   ΑΥΤΟΥΣΙΕΣ από το δέντρο (βλ. `noise_sample`).
//!
//! ΧΡΗΣΗ:  cargo run --release --bin ltass_band_matrix -- [q] [noise_map]
//!         q          default 1.0
//!         noise_map  default 0

use rustfft::{num_complex::Complex, FftPlanner};
use sp314_dsp::masking_eq::biquad::rbj_peaking_coeffs;

// ── ΤΟ ΟΡΓΑΝΟ — ΚΑΘΕ ΤΙΜΗ ΑΠΟ ΤΟ F-098, ΣΤΑΘΕΡΗ, ΟΧΙ CLI ──────────────
const SEED: u32 = 0x5EED1234;
const SR: u32 = 48_000;
const DURATION_SEC: usize = 30;
const NFFT: usize = 16_384;
const HOP: usize = NFFT / 2; // 50% overlap
const PROBE_DB: f32 = 6.0; // το σημείο λειτουργίας του πίνακα

/// dsp/mod.rs:426 — αυτούσιο.
const LTASS_CFS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];

/// pre_analysis.rs:108-110 — αυτούσιο. Ό,τι διαβάζει ο resolver.
const BAND_EDGES: [f32; 9] = [
    20.0, 80.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 20000.0,
];

// ── ΤΟ ΣΗΜΑ ──────────────────────────────────────────────────────────
/// Ο ΜΟΝΑΔΙΚΟΣ LCG του δέντρου: 1664525 / 1013904223.
#[inline]
fn lcg_next(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    *seed
}

/// Οι ΤΡΕΙΣ μετατροπές που ζουν στο δέντρο, αυτούσιες. Καμία δεν είναι
/// «η σωστή» — το F-098 δεν κατέγραψε ποια χρησιμοποιήθηκε.
///   0 — vad_sensors.rs:181   (seed >> 8) / 2^24 * 2 - 1
///   1 — crossover.rs:131     (seed >> 16) as i16 / 32768
///   2 — cut_heal/mod.rs:159  seed / u32::MAX * 2 - 1
#[inline]
fn noise_sample(seed: &mut u32, map: u8) -> f32 {
    let s = lcg_next(seed);
    match map {
        0 => (s >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0,
        1 => (s >> 16) as i16 as f32 / 32768.0,
        _ => (s as f32 / u32::MAX as f32) * 2.0 - 1.0,
    }
}

fn white_noise(n: usize, map: u8) -> Vec<f32> {
    let mut seed = SEED;
    (0..n).map(|_| noise_sample(&mut seed, map)).collect()
}

// ── Η ΑΛΥΣΙΔΑ ────────────────────────────────────────────────────────
/// Ένα peaking biquad, direct form I. Οι συντελεστές έρχονται από τη
/// ΓΕΝΝΗΤΡΙΑ ΤΟΥ REPO (`rbj_peaking_coeffs`), δεν ξαναγράφονται εδώ.
struct Peaking {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Peaking {
    fn new(center_hz: f32, gain_db: f32, q: f32) -> Self {
        let c = rbj_peaking_coeffs(center_hz, gain_db, q, SR);
        Self {
            b0: c[0],
            b1: c[1],
            b2: c[2],
            a1: c[3],
            a2: c[4],
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Τα οκτώ σε ΣΕΙΡΑ — όπως το `vocal_graph_topology`
/// (in → deesser → ltass_band_0 … ltass_band_7 → vca_gain → out).
fn run_chain(input: &[f32], gains_db: [f32; 8], q: f32) -> Vec<f32> {
    let mut chain: Vec<Peaking> = (0..8)
        .map(|i| Peaking::new(LTASS_CFS[i], gains_db[i], q))
        .collect();
    input
        .iter()
        .map(|&x| chain.iter_mut().fold(x, |acc, f| f.process(acc)))
        .collect()
}

// ── Η ΜΕΤΡΗΣΗ ────────────────────────────────────────────────────────
/// Welch: N=16384, Hann, 50% overlap. Επιστρέφει PSD ανά bin (γραμμική
/// ισχύς, χωρίς κανονικοποίηση — οι λόγοι είναι που μετράνε).
fn welch_psd(x: &[f32]) -> Vec<f64> {
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(NFFT);

    let hann: Vec<f64> = (0..NFFT)
        .map(|i| {
            let w = 2.0 * std::f64::consts::PI * i as f64 / NFFT as f64;
            0.5 - 0.5 * w.cos()
        })
        .collect();

    let bins = NFFT / 2 + 1;
    let mut acc = vec![0.0_f64; bins];
    let mut segments = 0usize;

    let mut pos = 0usize;
    while pos + NFFT <= x.len() {
        let mut buf: Vec<Complex<f64>> = (0..NFFT)
            .map(|i| Complex::new(x[pos + i] as f64 * hann[i], 0.0))
            .collect();
        fft.process(&mut buf);
        for (k, a) in acc.iter_mut().enumerate() {
            *a += buf[k].norm_sqr();
        }
        segments += 1;
        pos += HOP;
    }

    let inv = 1.0 / segments as f64;
    acc.iter().map(|v| v * inv).collect()
}

/// Ολοκλήρωμα ισχύος πάνω σε κάθε μία από τις οκτώ BAND_EDGES.
/// ΑΥΤΟ διαβάζει ο resolver — όχι την τιμή στην κεντρική συχνότητα.
fn band_powers(psd: &[f64]) -> [f64; 8] {
    let bin_hz = SR as f64 / NFFT as f64;
    let mut out = [0.0_f64; 8];
    for (k, o) in out.iter_mut().enumerate() {
        let lo = BAND_EDGES[k] as f64;
        let hi = BAND_EDGES[k + 1] as f64;
        let mut sum = 0.0;
        for (i, p) in psd.iter().enumerate() {
            let f = i as f64 * bin_hz;
            if f >= lo && f < hi {
                sum += p;
            }
        }
        *o = sum;
    }
    out
}

// ── ΓΡΑΜΜΙΚΗ ΑΛΓΕΒΡΑ (8×8, Gauss-Jordan με μερική οδήγηση) ───────────
fn invert(m: &[[f64; 8]; 8]) -> Option<([[f64; 8]; 8], f64)> {
    let mut a = *m;
    let mut inv = [[0.0_f64; 8]; 8];
    for (i, row) in inv.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    let mut det = 1.0_f64;

    for col in 0..8 {
        let mut piv = col;
        for r in col + 1..8 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        if piv != col {
            a.swap(piv, col);
            inv.swap(piv, col);
            det = -det;
        }
        let d = a[col][col];
        det *= d;
        for c in 0..8 {
            a[col][c] /= d;
            inv[col][c] /= d;
        }
        for r in 0..8 {
            if r == col {
                continue;
            }
            let f = a[r][col];
            if f == 0.0 {
                continue;
            }
            for c in 0..8 {
                a[r][c] -= f * a[col][c];
                inv[r][c] -= f * inv[col][c];
            }
        }
    }
    Some((inv, det))
}

/// ΔΗΛΩΜΕΝΟ: condition number σε ΑΠΕΙΡΟ-ΝΟΡΜΑ (max άθροισμα γραμμής).
/// Άλλη νόρμα δίνει άλλο νούμερο — αν συγκρίνεις, σύγκρινε ίδια νόρμα.
fn cond_inf(m: &[[f64; 8]; 8], inv: &[[f64; 8]; 8]) -> f64 {
    let norm = |x: &[[f64; 8]; 8]| {
        x.iter()
            .map(|r| r.iter().map(|v| v.abs()).sum::<f64>())
            .fold(0.0_f64, f64::max)
    };
    norm(m) * norm(inv)
}

/// Φασματική νόρμα ‖M‖₂ = σmax, με power iteration πάνω στο MᵀM.
fn spectral_norm(m: &[[f64; 8]; 8]) -> f64 {
    let mut v = [1.0_f64 / (8.0_f64).sqrt(); 8];
    let mut sigma = 0.0_f64;
    for _ in 0..500 {
        // w = M v ; u = Mᵀ w
        let mut w = [0.0_f64; 8];
        for (r, wr) in w.iter_mut().enumerate() {
            *wr = (0..8).map(|c| m[r][c] * v[c]).sum();
        }
        let mut u = [0.0_f64; 8];
        for (c, uc) in u.iter_mut().enumerate() {
            *uc = (0..8).map(|r| m[r][c] * w[r]).sum();
        }
        let n = u.iter().map(|x| x * x).sum::<f64>().sqrt();
        if n < 1e-300 {
            return 0.0;
        }
        for (vi, ui) in v.iter_mut().zip(u.iter()) {
            *vi = ui / n;
        }
        sigma = n.sqrt();
    }
    sigma
}

/// ΔΗΛΩΜΕΝΟ: cond₂ = σmax(B) · σmax(B⁻¹) = σmax/σmin. Διαφορετική
/// ποσότητα από το cond∞ παρακάτω — ίδιος πίνακας, άλλος ορισμός.
fn cond_2(m: &[[f64; 8]; 8], inv: &[[f64; 8]; 8]) -> f64 {
    spectral_norm(m) * spectral_norm(inv)
}

fn print_matrix(name: &str, m: &[[f64; 8]; 8]) {
    println!("\n{name}");
    print!("        ");
    for i in 0..8 {
        print!("  φ{i:<7}");
    }
    println!();
    for (k, row) in m.iter().enumerate() {
        print!("band {k}  ");
        for v in row.iter() {
            print!("{v:9.4}");
        }
        println!();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let q: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let map: u8 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    let n = DURATION_SEC * SR as usize;
    let noise = white_noise(n, map);

    println!("── ltass_band_matrix ──────────────────────────────────────────");
    println!("ΟΡΓΑΝΟ: λευκός θόρυβος, LCG seed 0x{SEED:08X}, noise_map={map}");
    println!("        {DURATION_SEC}s @ {SR} Hz · Welch N={NFFT}, Hann, 50% overlap");
    println!("        σύγκριση με flat αναφορά (ίδια αλυσίδα, gain_db=0)");
    println!("        q = {q}   ·   probe = +{PROBE_DB} dB");
    println!("LTASS_CFS  {LTASS_CFS:?}");
    println!("BAND_EDGES {BAND_EDGES:?}");

    // Η flat αναφορά: ΙΔΙΑ αλυσίδα, όλα τα φίλτρα στο 0 dB.
    // Όχι το ωμό σήμα — έτσι ό,τι κάνει η ίδια η αλυσίδα ακυρώνεται.
    let flat = band_powers(&welch_psd(&run_chain(&noise, [0.0; 8], q)));

    let mut b = [[0.0_f64; 8]; 8];
    for i in 0..8 {
        let mut gains = [0.0_f32; 8];
        gains[i] = PROBE_DB;
        let p = band_powers(&welch_psd(&run_chain(&noise, gains, q)));
        for k in 0..8 {
            let delivered_db = 10.0 * (p[k] / flat[k]).log10();
            b[k][i] = delivered_db / PROBE_DB as f64;
        }
    }

    print_matrix("ΠΙΝΑΚΑΣ B — στήλη i = φίλτρο i στα +6 dB· γραμμή k = μπάντα k", &b);

    print!("\nδιαγώνιος ");
    for k in 0..8 {
        print!("{:8.4}", b[k][k]);
    }
    let dmin = (0..8).map(|k| b[k][k]).fold(f64::INFINITY, f64::min);
    let dmax = (0..8).map(|k| b[k][k]).fold(f64::NEG_INFINITY, f64::max);
    println!("\n          εύρος {dmin:.4} … {dmax:.4}");

    match invert(&b) {
        None => println!("\nΟ ΠΙΝΑΚΑΣ ΔΕΝ ΑΝΤΙΣΤΡΕΦΕΤΑΙ — μηδενική οδήγηση."),
        Some((binv, det)) => {
            println!("\ndet(B)  = {det:.6}");
            // ΔΥΟ ΟΡΙΣΜΟΙ, ΚΑΙ ΟΙ ΔΥΟ ΤΥΠΩΝΟΝΤΑΙ: το F-098 κατέγραψε
            // «cond(B) = 4.84» ΧΩΡΙΣ να δηλώσει νόρμα. Χωρίς δήλωση, ένας
            // αριθμός συνθήκης δεν επαληθεύεται από τρίτον.
            println!("cond₂(B) = {:.4}   (φασματική, σmax/σmin)", cond_2(&b, &binv));
            println!("cond∞(B) = {:.4}   (άπειρο-νόρμα)", cond_inf(&b, &binv));
            print_matrix("ΠΙΝΑΚΑΣ B⁻¹", &binv);
            print!("\nδιαγώνιος B⁻¹ ");
            for k in 0..8 {
                print!("{:8.4}", binv[k][k]);
            }
            println!();
            // Το εφαρμοζόμενο κέρδος για ζητούμενο [+6, 0, …, 0] — δηλαδή
            // ΜΟΝΟ στη μπάντα 0. applied = B⁻¹·s ⇒ applied[0] = B⁻¹[0][0]·6.
            // Αυτή είναι η τιμή που το G_MAX_DB = 6.0 ψαλιδίζει (F-098).
            let applied_band0 = binv[0][0] * PROBE_DB as f64;
            println!(
                "\nζητούμενο [+{PROBE_DB},0,…,0] ⇒ ΕΦΑΡΜΟΓΗ στη μπάντα 0: {applied_band0:.3} dB\
                 \n  (G_MAX_DB = 6.0 το ψαλιδίζει — F-098, «το g_max σε τρίτη μονάδα»)"
            );
        }
    }
}
