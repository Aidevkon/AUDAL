//! ΜΕΤΡΗΣΗ 2026-08-24 — ο DeHum αφαιρεί hum ή φωνή;
//! READ+RUN, standalone research binary. ΔΕΝ αγγίζει production code —
//! καλεί το ΠΡΑΓΜΑΤΙΚΟ DeHumNode (sp314-nodes), όχι re-implementation.
//!
//! Follow-up του F-081: null −32/−30dB στον DeHum ήταν πιο δυνατό από
//! τον de-esser στα defaults του — ύποπτο για ένα «χειρουργικό» notch.
//! Εδώ: πού ΦΑΣΜΑΤΙΚΑ πάει η αφαιρεθείσα ενέργεια (ζώνες + κέντρο
//! βάρους, POWER-weighted) σε τρία πραγματικά αρχεία, ΚΑΙ ένα oracle
//! με γνωστή απάντηση (50Hz hum + 200Hz "φωνή").

use rustfft::{num_complex::Complex, FftPlanner};
use sp314_dsp::io::wav_writer::WavWriter;
use sp314_nodes::node::DspNode;
use sp314_nodes::nodes::dehum::DeHumNode;
use std::path::PathBuf;

const FRAME_SIZE: usize = 4096;
const HOP: usize = 2048;
const CLIP_SECS: f64 = 20.0;
const AUDITION_DIR: &str = "/tmp/dehum-audition";
// Ζώνη όπου ο DeHum μπορεί να δράσει (50/100/150Hz notches) — για την
// αναζήτηση του πυκνότερου 20άρι, ώστε η επίδραση (αν υπάρχει) να είναι
// πιο πιθανό να ακουστεί.
const SEARCH_LO_HZ: f64 = 0.0;
const SEARCH_HI_HZ: f64 = 250.0;

fn decode_stereo_native(path: &str) -> (Vec<f32>, Vec<f32>, u32) {
    let probe = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=sample_rate", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .expect("spawn ffprobe");
    let sr: u32 = String::from_utf8_lossy(&probe.stdout)
        .lines()
        .next()
        .expect("no sr")
        .trim()
        .parse()
        .expect("bad sr");

    let tmp = PathBuf::from(format!("/tmp/dehum_what_{}.pcm", std::process::id()));
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(path)
        .args(["-ac", "2", "-ar", &sr.to_string(), "-f", "f32le"])
        .arg(&tmp)
        .status()
        .expect("spawn ffmpeg");
    assert!(status.success(), "ffmpeg decode failed");
    let bytes = std::fs::read(&tmp).expect("read pcm");
    let _ = std::fs::remove_file(&tmp);
    let interleaved: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    let mut l = Vec::with_capacity(interleaved.len() / 2);
    let mut r = Vec::with_capacity(interleaved.len() / 2);
    for pair in interleaved.chunks_exact(2) {
        l.push(pair[0]);
        r.push(pair[1]);
    }
    (l, r, sr)
}

fn rms_db(v: &[f32]) -> f64 {
    if v.is_empty() {
        return f64::NEG_INFINITY;
    }
    let sum_sq: f64 = v.iter().map(|&x| (x as f64) * (x as f64)).sum();
    let rms = (sum_sq / v.len() as f64).sqrt();
    if rms < 1e-12 {
        -240.0
    } else {
        20.0 * rms.log10()
    }
}

fn hann_window(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n as f32 - 1.0)).cos())
        .collect()
}

/// Power spectrum (bins 0..=N/2, |X_k|^2) per frame, Hann-windowed.
fn stft_power(signal: &[f32], frame_size: usize, hop: usize) -> Vec<Vec<f64>> {
    if signal.len() < frame_size {
        return Vec::new();
    }
    let window = hann_window(frame_size);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(frame_size);
    let n_frames = (signal.len() - frame_size) / hop + 1;
    let mut out = Vec::with_capacity(n_frames);
    let mut buf: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); frame_size];
    for i in 0..n_frames {
        let start = i * hop;
        for j in 0..frame_size {
            buf[j] = Complex::new(signal[start + j] * window[j], 0.0);
        }
        fft.process(&mut buf);
        let pow: Vec<f64> = buf[..=frame_size / 2].iter().map(|c| (c.norm() as f64).powi(2)).collect();
        out.push(pow);
    }
    out
}

struct ZoneReport {
    zone_pct: [f64; 8], // <40,40-70,70-130,130-250,250-500,500-2k,2k-6k,>6k
    centroid_hz: f64,
}

const ZONE_EDGES_HZ: [f64; 9] = [0.0, 40.0, 70.0, 130.0, 250.0, 500.0, 2000.0, 6000.0, f64::INFINITY];
const ZONE_NAMES: [&str; 8] = ["<40Hz", "40-70Hz", "70-130Hz", "130-250Hz", "250-500Hz", "500-2kHz", "2k-6kHz", ">6kHz"];

fn analyze_zones(diff_mono: &[f32], sr: u32) -> ZoneReport {
    let frames = stft_power(diff_mono, FRAME_SIZE, HOP);
    let n_bins = FRAME_SIZE / 2 + 1;
    let mut accum = vec![0.0_f64; n_bins];
    for frame in &frames {
        for (k, &p) in frame.iter().enumerate() {
            accum[k] += p;
        }
    }
    let bin_hz = sr as f64 / FRAME_SIZE as f64;
    let total: f64 = accum.iter().sum();

    let mut zone_pct = [0.0_f64; 8];
    if total > 1e-30 {
        for (k, &p) in accum.iter().enumerate() {
            let f = k as f64 * bin_hz;
            for z in 0..8 {
                if f >= ZONE_EDGES_HZ[z] && f < ZONE_EDGES_HZ[z + 1] {
                    zone_pct[z] += p;
                    break;
                }
            }
        }
        for z in zone_pct.iter_mut() {
            *z = *z / total * 100.0;
        }
    }

    let num: f64 = accum.iter().enumerate().map(|(k, &p)| k as f64 * bin_hz * p).sum();
    let centroid_hz = if total > 1e-30 { num / total } else { f64::NAN };

    ZoneReport { zone_pct, centroid_hz }
}

/// Densest <250Hz-energy 20s window, searched on the UNPROCESSED mono downmix.
fn find_densest_lowfreq_window(mono: &[f32], sr: u32) -> usize {
    let frames = stft_power(mono, FRAME_SIZE, HOP);
    if frames.is_empty() {
        return 0;
    }
    let bin_hz = sr as f64 / FRAME_SIZE as f64;
    let lo_bin = (SEARCH_LO_HZ / bin_hz).floor() as usize;
    let hi_bin = ((SEARCH_HI_HZ / bin_hz).ceil() as usize).min(frames[0].len() - 1);
    let frame_band_energy: Vec<f64> = frames.iter().map(|f| f[lo_bin..=hi_bin].iter().sum()).collect();

    let window_frames = ((CLIP_SECS * sr as f64 / HOP as f64).round() as usize).max(1);
    if frame_band_energy.len() <= window_frames {
        return 0;
    }
    let mut window_sum: f64 = frame_band_energy[..window_frames].iter().sum();
    let mut best_sum = window_sum;
    let mut best_start_frame = 0usize;
    for start in 1..=(frame_band_energy.len() - window_frames) {
        window_sum += frame_band_energy[start + window_frames - 1] - frame_band_energy[start - 1];
        if window_sum > best_sum {
            best_sum = window_sum;
            best_start_frame = start;
        }
    }
    best_start_frame * HOP
}

fn slice_clip(l: &[f32], r: &[f32], start: usize, sr: u32) -> (Vec<f32>, Vec<f32>) {
    let clip_len = (CLIP_SECS * sr as f64).round() as usize;
    let start = start.min(l.len().saturating_sub(clip_len));
    let end = (start + clip_len).min(l.len());
    (l[start..end].to_vec(), r[start..end].to_vec())
}

fn new_poxvoice_dehum(sr: u32) -> DeHumNode {
    let mut n = DeHumNode::new(sr);
    n.set_parameter("enabled", 1.0);
    n.set_parameter("fundamental_hz", 50.0);
    // "harmonics" ΔΕΝ γίνεται override στο POXVoice — μένει constructor
    // default 3.0 (βλ. flavor.rs:164).
    n
}

/// Single-tone amplitude via quadrature (Goertzel-style) correlation over
/// the whole signal — exact για καθαρό ημίτονο, χωρίς FFT bin leakage.
fn tone_amplitude(signal: &[f32], freq_hz: f64, sr: u32) -> f64 {
    let n = signal.len();
    let mut i_sum = 0.0_f64;
    let mut q_sum = 0.0_f64;
    for (n_idx, &s) in signal.iter().enumerate() {
        let phase = 2.0 * std::f64::consts::PI * freq_hz * n_idx as f64 / sr as f64;
        i_sum += s as f64 * phase.cos();
        q_sum += s as f64 * phase.sin();
    }
    2.0 * (i_sum * i_sum + q_sum * q_sum).sqrt() / n as f64
}

fn run_oracle(sr: u32) {
    println!("=== ORACLE: 50Hz@-40dBFS + 200Hz@-20dBFS, μέσα από DeHum (POXVoice params) ===");
    let dur_s = 5.0_f64;
    let n = (dur_s * sr as f64) as usize;
    let amp50 = 10f64.powf(-40.0 / 20.0);
    let amp200 = 10f64.powf(-20.0 / 20.0);
    let mut l0: Vec<f32> = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / sr as f64;
        let s = amp50 * (2.0 * std::f64::consts::PI * 50.0 * t).sin()
            + amp200 * (2.0 * std::f64::consts::PI * 200.0 * t).sin();
        l0.push(s as f32);
    }
    let r0 = l0.clone();

    // Μέτρηση σε steady-state — πετάμε το πρώτο 1s (φίλτρο settling).
    let skip = (1.0 * sr as f64) as usize;
    let in50 = tone_amplitude(&l0[skip..], 50.0, sr);
    let in200 = tone_amplitude(&l0[skip..], 200.0, sr);

    let mut l = l0.clone();
    let mut r = r0.clone();
    let mut node = new_poxvoice_dehum(sr);
    node.process_stereo(&mut l, &mut r);

    let out50 = tone_amplitude(&l[skip..], 50.0, sr);
    let out200 = tone_amplitude(&l[skip..], 200.0, sr);

    let atten50_db = 20.0 * (out50 / in50).log10();
    let atten200_db = 20.0 * (out200 / in200).log10();

    println!("  input:  50Hz amp={in50:.6} ({:.2} dBFS)   200Hz amp={in200:.6} ({:.2} dBFS)",
        20.0 * in50.log10(), 20.0 * in200.log10());
    println!("  output: 50Hz amp={out50:.6} ({:.2} dBFS)   200Hz amp={out200:.6} ({:.2} dBFS)",
        20.0 * out50.log10(), 20.0 * out200.log10());
    println!("  50Hz attenuation  = {atten50_db:.2} dB");
    println!("  200Hz attenuation = {atten200_db:.2} dB");
    println!();
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    assert!(!paths.is_empty(), "usage: dehum_what <file1> [file2] ...");
    std::fs::create_dir_all(AUDITION_DIR).expect("mkdir audition dir");

    run_oracle(44100);

    let mut audition_entries: Vec<String> = Vec::new();

    for path in &paths {
        println!("══════════════════════════════════════════════════════");
        println!("input: {path}");
        let (l0, r0, sr) = decode_stereo_native(path);
        println!("sr={sr} samples={} dur={:.2}s", l0.len(), l0.len() as f64 / sr as f64);

        let mut l = l0.clone();
        let mut r = r0.clone();
        let mut node = new_poxvoice_dehum(sr);
        node.process_stereo(&mut l, &mut r);

        let mut diff_combined = Vec::with_capacity(l.len() * 2);
        let mut dl = Vec::with_capacity(l.len());
        let mut dr = Vec::with_capacity(r.len());
        for i in 0..l.len() {
            let vl = l0[i] - l[i];
            let vr = r0[i] - r[i];
            diff_combined.push(vl);
            diff_combined.push(vr);
            dl.push(vl);
            dr.push(vr);
        }
        let null_rms_db = rms_db(&diff_combined);
        println!("null_rms_db (in-out) = {null_rms_db:.4}");

        let diff_mono: Vec<f32> = dl.iter().zip(dr.iter()).map(|(&a, &b)| (a + b) * 0.5).collect();
        let zr = analyze_zones(&diff_mono, sr);
        println!("κέντρο βάρους αφαιρεθέντος = {:.1} Hz", zr.centroid_hz);
        println!("ζώνες (% της αφαιρεθείσας ενέργειας, POWER-weighted):");
        for i in 0..8 {
            println!("  {:>10}: {:>6.2}%", ZONE_NAMES[i], zr.zone_pct[i]);
        }
        let above_250: f64 = zr.zone_pct[4] + zr.zone_pct[5] + zr.zone_pct[6] + zr.zone_pct[7];
        println!("ΠΟΣΟΣΤΟ ΠΑΝΩ ΑΠΟ 250Hz = {above_250:.2}%  <-- ΤΟ ΚΡΙΣΙΜΟ ΝΟΥΜΕΡΟ");
        println!();

        // Audition clip: densest <250Hz window.
        let mono0: Vec<f32> = l0.iter().zip(r0.iter()).map(|(&a, &b)| (a + b) * 0.5).collect();
        let clip_start = find_densest_lowfreq_window(&mono0, sr);
        let clip_start_s = clip_start as f64 / sr as f64;

        let stub: String = std::path::Path::new(path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
            .collect::<String>();
        let stub = stub.trim_matches('-').to_string();

        let (ul, ur) = slice_clip(&l0, &r0, clip_start, sr);
        let unproc_path = format!("{AUDITION_DIR}/{stub}__unprocessed.wav");
        WavWriter::write(&unproc_path, &ul, &ur, sr).expect("write unprocessed clip");

        let (pl, pr) = slice_clip(&l, &r, clip_start, sr);
        let dehum_path = format!("{AUDITION_DIR}/{stub}__dehum.wav");
        WavWriter::write(&dehum_path, &pl, &pr, sr).expect("write dehum clip");

        audition_entries.push(format!(
            "| {stub} | ΑΝΕΠΕΞΕΡΓΑΣΤΟ | t={clip_start_s:.2}s..{:.2}s | `{unproc_path}` |",
            clip_start_s + CLIP_SECS
        ));
        audition_entries.push(format!(
            "| {stub} | DeHum (POXVoice params) | t={clip_start_s:.2}s..{:.2}s | `{dehum_path}` |",
            clip_start_s + CLIP_SECS
        ));
        println!("densest <250Hz 20s window: t={clip_start_s:.2}s -> {unproc_path} / {dehum_path}");
        println!();
    }

    let mut md = String::new();
    md.push_str("# DeHum audition clips (unprocessed / dehum pairs)\n\n");
    md.push_str("Ζευγάρια A/B: το πρώτο ΑΝΕΠΕΞΕΡΓΑΣΤΟ, το δεύτερο μέσα από DeHum (POXVoice params).\n\n");
    md.push_str("| αρχείο | παραλλαγή | εύρος | path |\n");
    md.push_str("|---|---|---|---|\n");
    for e in &audition_entries {
        md.push_str(e);
        md.push('\n');
    }
    std::fs::write(format!("{AUDITION_DIR}/audition.md"), md).expect("write audition.md");
    println!("audition.md γράφτηκε στο {AUDITION_DIR}/audition.md");
}
