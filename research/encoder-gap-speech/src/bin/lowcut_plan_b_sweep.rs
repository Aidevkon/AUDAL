//! Σχέδιο Β σε render: γωνία = μετρημένη θεμελιώδης / 2, ανά αρχείο
//! (πίνακας της 15/09, docs/lab-logs/lowcut-rumble-20260915.txt). ΙΔΙΟ
//! εργαλείο/τύπος φίλτρου με lowcut_corner_sweep.rs (Σχέδιο Α) — μόνο η
//! γωνία αλλάζει ανά αρχείο αντί να είναι σταθερή. janeeyre ΕΞΑΙΡΕΙΤΑΙ
//! (καμία μετρημένη θεμελιώδης — όριο του σχεδίου, όχι του τρεξίματος).
//!
//! ΜΗΔΕΝ αλλαγή παραγωγής (cleaner.rs:69 παραμένει hardcoded 80Hz —
//! εδώ εφαρμόζεται ισοδύναμο φίλτρο απευθείας, ίδιο idiom με το Α).
//! ΜΗΔΕΝ κατώφλι, ΜΗΔΕΝ επιλογή σχεδίου.

const SR: f32 = 48_000.0;
const NFFT: usize = 16_384;

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

fn write_dump_stereo(dump: &str, l: &[f32], r: &[f32]) {
    let mut buf = Vec::with_capacity(l.len() * 8);
    for i in 0..l.len() {
        buf.extend_from_slice(&l[i].to_le_bytes());
        buf.extend_from_slice(&r[i].to_le_bytes());
    }
    std::fs::write(dump, buf).expect("write dump");
}

/// ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ, ίδιο με lowcut_corner_sweep.rs (RBJ HighPass,
/// ίδιο με biquad.rs, μεταβλητή γωνία).
struct HpBiquad {
    b0: f32, b1: f32, b2: f32, a1: f32, a2: f32,
    z1_l: f32, z2_l: f32, z1_r: f32, z2_r: f32,
}
impl HpBiquad {
    fn new(freq: f32, q: f32, sr: f32) -> Self {
        let omega = 2.0 * std::f32::consts::PI * freq / sr;
        let alpha = omega.sin() / (2.0 * q);
        let cos_w = omega.cos();
        let b0 = (1.0 + cos_w) / 2.0;
        let b1 = -(1.0 + cos_w);
        let b2 = (1.0 + cos_w) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w;
        let a2 = 1.0 - alpha;
        Self { b0: b0/a0, b1: b1/a0, b2: b2/a0, a1: a1/a0, a2: a2/a0, z1_l:0.0, z2_l:0.0, z1_r:0.0, z2_r:0.0 }
    }
    fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for i in 0..l.len() {
            let out_l = self.b0 * l[i] + self.z1_l;
            self.z1_l = self.b1 * l[i] - self.a1 * out_l + self.z2_l;
            self.z2_l = self.b2 * l[i] - self.a2 * out_l;
            let out_r = self.b0 * r[i] + self.z1_r;
            self.z1_r = self.b1 * r[i] - self.a1 * out_r + self.z2_r;
            self.z2_r = self.b2 * r[i] - self.a2 * out_r;
            l[i] = out_l;
            r[i] = out_r;
        }
    }
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
    if segs == 0 { return vec![-200.0; NFFT / 2 + 1]; }
    acc.iter().map(|&v| { let m = v / segs as f64; if m < 1e-30 { -300.0 } else { (10.0*m.log10()) as f32 } }).collect()
}

fn hz_to_bin(hz: f32) -> usize { (hz / (SR / NFFT as f32)).round() as usize }
fn peak_db_near(psd: &[f32], target_hz: f32, tol_hz: f32) -> f32 {
    let lo = hz_to_bin((target_hz - tol_hz).max(0.0));
    let hi = hz_to_bin(target_hz + tol_hz).min(psd.len() - 1);
    (lo..=hi).map(|i| psd[i]).fold(f32::NEG_INFINITY, f32::max)
}

fn main() {
    let base = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    // (όνομα, γωνία_Β = θεμ./2, θέση παράθυρο ομιλίας, διάρκεια, θεμελιώδης)
    // — ΟΛΑ αυτούσια από το lab-log. janeeyre ΔΕΝ έχει γραμμή: καμία
    // θεμελιώδης μετρήθηκε (καμία τομή Otsu) — όριο του σχεδίου.
    let files: [(&str, f32, f32, f32, f32); 8] = [
        ("secretgarden_01_burnett", 123.045, 121.9, 3.00, 246.09),
        ("dracula_01_stoker", 105.47, 210.3, 3.00, 210.94),
        ("huckfinn_01_twain_apc", 106.935, 214.0, 3.00, 213.87),
        ("peterpan_01_barrie", 123.045, 433.1, 3.00, 246.09),
        ("tale_of_two_cities_01_dickens", 55.665, 263.4, 3.00, 111.33),
        ("adventurespinocchio_01_collodi", 104.005, 160.0, 3.00, 208.01),
        ("anne_of_green_gables_01_montgomery", 104.005, 697.5, 3.00, 208.01),
        ("count_of_monte_cristo_001_dumas", 106.935, 338.7, 3.00, 213.87),
    ];

    println!("janeeyre_01_bronte: ΕΞΑΙΡΕΙΤΑΙ — καμία μετρημένη θεμελιώδης (καμία τομή Otsu). Όριο του Σχεδίου Β, όχι του τρεξίματος.\n");
    println!("{:<38} {:>8} {:>12} {:>12} {:>9} {:>7} {:>9} {:>9}",
        "αρχείο", "γωνίαΒ", "interior OFF", "interior ON", "Δ", "<-60;", "ΔB1", "Δλόγος");

    let mut pass_count = 0usize;
    for (name, corner, start_s, dur_s, fund_hz) in files {
        let path = format!("{base}/{name}.mp3");
        let off_dump = decode_dump(&path, &format!("{name}_planb"));
        let off_report = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
            std::path::Path::new(&off_dump), false, 5.0,
        ).expect("trunk off");
        let (l0, r0) = read_dump_stereo(&off_dump);

        let mut hp = HpBiquad::new(corner, 0.707, SR);
        let mut l1 = l0.clone();
        let mut r1 = r0.clone();
        hp.process(&mut l1, &mut r1);

        let on_dump = format!("/tmp/lowcut_rumble_{name}_planb_on.raw");
        write_dump_stereo(&on_dump, &l1, &r1);
        let on_report = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
            std::path::Path::new(&on_dump), false, 5.0,
        ).expect("trunk on");

        let off_i = off_report.acx_interior_noise_floor.map(|(db, _)| db);
        let on_i = on_report.acx_interior_noise_floor.map(|(db, _)| db);
        let (d_i, passes) = match (off_i, on_i) {
            (Some(o), Some(n)) => (Some(n - o), n <= -60.0),
            _ => (None, false),
        };
        if passes {
            pass_count += 1;
        }

        let d_b1 = on_report.spectral_profile_db[1] - off_report.spectral_profile_db[1];

        // ── λόγος θεμελιώδους/2ης αρμονικής, στο ΙΔΙΟ παράθυρο ──
        let start_frame = (start_s * SR) as usize;
        let len_frame = (dur_s * SR) as usize;
        let end_frame = (start_frame + len_frame).min(l0.len());
        let mono_off: Vec<f32> = l0[start_frame..end_frame].iter().zip(&r0[start_frame..end_frame]).map(|(a,b)| (a+b)*0.5).collect();
        let mono_on: Vec<f32> = l1[start_frame..end_frame].iter().zip(&r1[start_frame..end_frame]).map(|(a,b)| (a+b)*0.5).collect();
        let psd_off = welch(&mono_off);
        let psd_on = welch(&mono_on);
        let h2_hz = fund_hz * 2.0;
        let f_off = peak_db_near(&psd_off, fund_hz, 3.0);
        let f_on = peak_db_near(&psd_on, fund_hz, 3.0);
        let h_off = peak_db_near(&psd_off, h2_hz, 3.0);
        let h_on = peak_db_near(&psd_on, h2_hz, 3.0);
        let ratio_off = f_off - h_off;
        let ratio_on = f_on - h_on;
        let d_ratio = ratio_on - ratio_off;

        match d_i {
            Some(d) => println!(
                "{:<38} {:>8.2} {:>12.3} {:>12.3} {:>9.3} {:>7} {:>9.3} {:>9.3}",
                name, corner, off_i.unwrap(), on_i.unwrap(), d, if passes { "ΝΑΙ" } else { "οχι" }, d_b1, d_ratio
            ),
            None => println!("{:<38} {:>8.2} ΑΠΟΝ", name, corner),
        }

        let _ = std::fs::remove_file(&off_dump);
        let _ = std::fs::remove_file(&on_dump);
    }

    println!("\nΠΕΡΝΑΝΕ ΤΟ −60 ΜΕ ΣΧΕΔΙΟ Β: {pass_count}/8 (janeeyre εξαιρείται, δεν μετράει ούτε ως perno ούτε ως όχι)");
    println!("ΓΙΑ ΣΥΓΚΡΙΣΗ, 80Hz (πρώτο log): 5/9 (secretgarden, janeeyre, peterpan, tale, monte_cristo)");
}
