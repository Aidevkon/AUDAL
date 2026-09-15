//! Λόγος θεμελιώδους/2ης αρμονικής, OFF/ON, στο ΙΔΙΟ παράθυρο ομιλίας
//! που ήδη βρήκε το Σημείο Β (lowcut_rumble_audition.rs, docs/lab-logs/
//! lowcut-rumble-20260915.txt) — θέσεις/θεμελιώδεις ΑΥΤΟΥΣΙΕΣ από εκεί,
//! ΔΕΝ ξανα-υπολογίζονται. Υπόθεση προς έλεγχο: η απώλεια «βάθους» στο
//! tale δεν είναι στάθμη (η θεμελιώδης έπεσε μόλις -1.02dB) αλλά
//! ισορροπία θεμελιώδους/αρμονικής.
//!
//! welch()/bin_hz(): ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ από lowcut_rumble_audition.rs
//! (το οποίο το είχε ήδη αντιγράψει-δηλωμένο από hum_spectrum.rs).
//!
//! ΜΗΔΕΝ αλλαγή στο εργαλείο, στο φίλτρο, ή στη λωρίδα. ΜΗΔΕΝ κατώφλι.

use sp314_dsp::restoration::{RestorationChain, RestorationConfig};

const NFFT: usize = 16_384;
const SR: f32 = 48_000.0;

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
            bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3],
        ]));
    }
    let l: Vec<f32> = inter.chunks_exact(2).map(|f| f[0]).collect();
    let r: Vec<f32> = inter.chunks_exact(2).map(|f| f[1]).collect();
    (l, r)
}

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
            if m < 1e-30 { -300.0 } else { (10.0 * m.log10()) as f32 }
        })
        .collect()
}

fn bin_hz(k: usize) -> f32 {
    k as f32 * SR / NFFT as f32
}

fn hz_to_bin(hz: f32) -> usize {
    (hz / (SR / NFFT as f32)).round() as usize
}

/// Κορυφή σε ±tol_hz γύρω από target_hz, ΧΩΡΙΣ όριο LO/HI — χρειάζεται
/// να φτάσει πέρα από τα 400Hz του hum_spectrum-εύρους για τις 2ες
/// αρμονικές αρκετών αρχείων εδώ.
fn peak_db_near(psd: &[f32], target_hz: f32, tol_hz: f32) -> f32 {
    let lo = hz_to_bin((target_hz - tol_hz).max(0.0));
    let hi = hz_to_bin(target_hz + tol_hz).min(psd.len() - 1);
    (lo..=hi).map(|i| psd[i]).fold(f32::NEG_INFINITY, f32::max)
}

fn main() {
    let base = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    // (όνομα, prom_ref, start_s, dur_s, fund_hz) — ΟΛΑ αυτούσια από
    // docs/lab-logs/lowcut-rumble-20260915.txt, Σημείο Β. janeeyre ΑΠΟΝ
    // εκεί (καμία τομή Otsu) — δεν έχει γραμμή εδώ.
    let files: [(&str, f32, f32, f32, f32); 8] = [
        ("secretgarden_01_burnett", 20.96, 121.9, 3.00, 246.09),
        ("dracula_01_stoker", 19.77, 210.3, 3.00, 210.94),
        ("huckfinn_01_twain_apc", 4.48, 214.0, 3.00, 213.87),
        ("peterpan_01_barrie", 3.52, 433.1, 3.00, 246.09),
        ("tale_of_two_cities_01_dickens", 3.49, 263.4, 3.00, 111.33),
        ("adventurespinocchio_01_collodi", f32::NAN, 160.0, 3.00, 208.01),
        ("anne_of_green_gables_01_montgomery", f32::NAN, 697.5, 3.00, 208.01),
        ("count_of_monte_cristo_001_dumas", f32::NAN, 338.7, 3.00, 213.87),
    ];

    println!("{:<38} {:>9} {:>9} {:>10} {:>10} {:>9} {:>9} {:>9}",
        "αρχείο", "fund_Hz", "2nd_Hz", "λόγος_OFF", "λόγος_ON", "Δλόγος", "θεμ.Δ", "Δλόγος-θεμ.Δ");

    for (name, _prom, start_s, dur_s, fund_hz) in files {
        let path = format!("{base}/{name}.mp3");
        let off_dump = decode_dump(&path, &format!("{name}_harm"));
        let (l0, r0) = read_dump_stereo(&off_dump);

        let mut chain_on = RestorationChain::new(
            SR,
            RestorationConfig { lowcut_enabled: true, hum_enabled: false, deess_enabled: false, gate_enabled: false },
            0.0,
            f32::NEG_INFINITY,
        );
        let mut l1 = l0.clone();
        let mut r1 = r0.clone();
        chain_on.process(&mut l1, &mut r1);

        let start_frame = (start_s * SR) as usize;
        let len_frame = (dur_s * SR) as usize;
        let end_frame = (start_frame + len_frame).min(l0.len());

        let mono_off: Vec<f32> = l0[start_frame..end_frame].iter().zip(&r0[start_frame..end_frame]).map(|(a, b)| (a + b) * 0.5).collect();
        let mono_on: Vec<f32> = l1[start_frame..end_frame].iter().zip(&r1[start_frame..end_frame]).map(|(a, b)| (a + b) * 0.5).collect();

        let psd_off = welch(&mono_off);
        let psd_on = welch(&mono_on);

        let fund_off = peak_db_near(&psd_off, fund_hz, 3.0);
        let fund_on = peak_db_near(&psd_on, fund_hz, 3.0);
        let h2_hz = fund_hz * 2.0;
        let h2_off = peak_db_near(&psd_off, h2_hz, 3.0);
        let h2_on = peak_db_near(&psd_on, h2_hz, 3.0);

        let ratio_off = fund_off - h2_off;
        let ratio_on = fund_on - h2_on;
        let d_ratio = ratio_on - ratio_off;
        let d_fund = fund_on - fund_off;

        println!(
            "{:<38} {:>9.2} {:>9.2} {:>10.2} {:>10.2} {:>9.3} {:>9.3} {:>13.3}",
            name, fund_hz, h2_hz, ratio_off, ratio_on, d_ratio, d_fund, d_ratio - d_fund
        );

        let _ = std::fs::remove_file(&off_dump);
    }
}
