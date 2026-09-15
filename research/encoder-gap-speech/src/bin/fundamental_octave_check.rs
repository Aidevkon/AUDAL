//! ΒΗΜΑ 0 — ΠΥΛΗ: το «μέγιστο bin 70-250Hz» της 15/09 βρήκε αρμονική ή
//! θεμελιώδη; Για κάθε ένα από τα οκτώ (janeeyre ΑΠΟΝ, καμία τομή Otsu),
//! στο ΙΔΙΟ παράθυρο ομιλίας/OFF, ελέγχει αν υπάρχει σαφής κορυφή στο
//! μισό και στο ένα τρίτο της βρεθείσας συχνότητας.
//!
//! welch()/local_floor(): ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ από hum_spectrum.rs
//! (ίδιος ορισμός τοπικού μέσου: διάμεσος σε ±25Hz, εξαιρουμένων ±4Hz).
//!
//! ΜΗΔΕΝ αλλαγή παραγωγής, φίλτρου, λωρίδας. ΜΗΔΕΝ κατώφλι.

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

fn hz_to_bin(hz: f32) -> usize {
    (hz / (SR / NFFT as f32)).round() as usize
}

fn peak_db_near(psd: &[f32], target_hz: f32, tol_hz: f32) -> f32 {
    let lo = hz_to_bin((target_hz - tol_hz).max(0.0));
    let hi = hz_to_bin(target_hz + tol_hz).min(psd.len() - 1);
    (lo..=hi).map(|i| psd[i]).fold(f32::NEG_INFINITY, f32::max)
}

/// ΙΔΙΟΣ ορισμός με hum_spectrum.rs local_floor: διάμεσος σε ±25Hz,
/// εξαιρουμένων ±4Hz γύρω από την κορυφή.
fn local_floor(psd: &[f32], target_hz: f32) -> f32 {
    let bin_hz_step = SR / NFFT as f32;
    let k = hz_to_bin(target_hz);
    let span = (25.0 / bin_hz_step) as usize;
    let skip = (4.0 / bin_hz_step) as usize;
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

/// Είναι το bin στο target_hz ΤΟΠΙΚΟ ΜΕΓΙΣΤΟ (>= και τους δύο γείτονες
/// σε ±1 bin);
fn is_local_peak(psd: &[f32], target_hz: f32) -> bool {
    let k = hz_to_bin(target_hz);
    if k == 0 || k + 1 >= psd.len() {
        return false;
    }
    psd[k] >= psd[k - 1] && psd[k] >= psd[k + 1]
}

fn main() {
    let base = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    // (όνομα, start_s, dur_s, fund_hz βρεθείσα 15/09) — αυτούσια.
    let files: [(&str, f32, f32, f32); 8] = [
        ("secretgarden_01_burnett", 121.9, 3.00, 246.09),
        ("dracula_01_stoker", 210.3, 3.00, 210.94),
        ("huckfinn_01_twain_apc", 214.0, 3.00, 213.87),
        ("peterpan_01_barrie", 433.1, 3.00, 246.09),
        ("tale_of_two_cities_01_dickens", 263.4, 3.00, 111.33),
        ("adventurespinocchio_01_collodi", 160.0, 3.00, 208.01),
        ("anne_of_green_gables_01_montgomery", 697.5, 3.00, 208.01),
        ("count_of_monte_cristo_001_dumas", 338.7, 3.00, 213.87),
    ];

    println!("{:<38} {:>9} | {:>9} {:>9} {:>6} | {:>9} {:>9} {:>6}",
        "αρχείο", "βρεθ.Hz", "μισό_Hz", "πάνω_dB", "peak;", "τρίτο_Hz", "πάνω_dB", "peak;");

    for (name, start_s, dur_s, fund_hz) in files {
        let path = format!("{base}/{name}.mp3");
        let off_dump = decode_dump(&path, &format!("{name}_gate"));
        let (l0, r0) = read_dump_stereo(&off_dump);

        let start_frame = (start_s * SR) as usize;
        let len_frame = (dur_s * SR) as usize;
        let end_frame = (start_frame + len_frame).min(l0.len());
        let mono_off: Vec<f32> = l0[start_frame..end_frame].iter().zip(&r0[start_frame..end_frame]).map(|(a, b)| (a + b) * 0.5).collect();
        let psd_off = welch(&mono_off);

        let half_hz = fund_hz / 2.0;
        let third_hz = fund_hz / 3.0;

        let half_db = peak_db_near(&psd_off, half_hz, 2.0);
        let half_floor = local_floor(&psd_off, half_hz);
        let half_above = half_db - half_floor;
        let half_peak = is_local_peak(&psd_off, half_hz);

        let third_db = peak_db_near(&psd_off, third_hz, 2.0);
        let third_floor = local_floor(&psd_off, third_hz);
        let third_above = third_db - third_floor;
        let third_peak = is_local_peak(&psd_off, third_hz);

        println!(
            "{:<38} {:>9.2} | {:>9.2} {:>9.2} {:>6} | {:>9.2} {:>9.2} {:>6}",
            name, fund_hz,
            half_hz, half_above, if half_peak { "ΝΑΙ" } else { "οχι" },
            third_hz, third_above, if third_peak { "ΝΑΙ" } else { "οχι" },
        );

        let _ = std::fs::remove_file(&off_dump);
    }
}
