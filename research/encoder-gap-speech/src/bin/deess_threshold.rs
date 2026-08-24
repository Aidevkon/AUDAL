//! ΜΕΤΡΗΣΗ 2026-08-24 — ποιο de-esser threshold δρα ΣΩΣΤΑ σε αφήγηση;
//! READ+RUN, standalone research binary. ΔΕΝ αγγίζει production code —
//! καλεί το ΠΡΑΓΜΑΤΙΚΟ DeEsserNode (sp314-nodes) και τον ΠΡΑΓΜΑΤΙΚΟ
//! WavWriter (sp314-dsp::io::wav_writer), όχι re-implementation.
//!
//! Follow-up του F-081: το production threshold (0.0) είναι δομικά
//! νεκρό, το default (−24.0) δρα (null −37dB) αλλά «δρα» ≠ «σωστά».
//! Εδώ μετράμε ΠΟΥ φασματικά αφαιρείται η ενέργεια (κέντρο βάρους) και
//! ΠΟΣΟ ΣΥΧΝΑ, σε 4 thresholds, ΚΑΙ γράφουμε clips ακρόασης.
//!
//! Νόμος 3: ένα standalone binary, ένα τρέξιμο ανά (αρχείο,threshold),
//! καμία cp-αλυσίδα.

use rustfft::{num_complex::Complex, FftPlanner};
use sp314_dsp::io::wav_writer::WavWriter;
use sp314_nodes::node::DspNode;
use sp314_nodes::nodes::deesser::DeEsserNode;
use std::path::PathBuf;

const FRAME_SIZE: usize = 4096;
const HOP: usize = 2048;
const ACT_CHUNK_MS: f64 = 10.0; // activation chunk size
const ACT_DBFS_THRESHOLD: f64 = -60.0;
const BAND_LO_HZ: f64 = 5000.0;
const BAND_HI_HZ: f64 = 9000.0;
const CLIP_SECS: f64 = 20.0;
const AUDITION_DIR: &str = "/tmp/deess-audition";

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

    let tmp = PathBuf::from(format!("/tmp/deess_threshold_{}.pcm", std::process::id()));
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

/// Magnitude spectrum (bins 0..=N/2) per frame, Hann-windowed, hop-spaced.
fn stft_magnitudes(signal: &[f32], frame_size: usize, hop: usize) -> Vec<Vec<f32>> {
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
        let mag: Vec<f32> = buf[..=frame_size / 2].iter().map(|c| c.norm()).collect();
        out.push(mag);
    }
    out
}

/// Energy-weighted spectral centroid (Hz) of the SUM of magnitude spectra
/// across all frames — "κέντρο βάρους" of the accumulated spectral content.
fn spectral_centroid_hz(frames: &[Vec<f32>], sr: u32, frame_size: usize) -> f64 {
    if frames.is_empty() {
        return f64::NAN;
    }
    let n_bins = frame_size / 2 + 1;
    let mut accum = vec![0.0_f64; n_bins];
    for frame in frames {
        for (k, &m) in frame.iter().enumerate() {
            accum[k] += m as f64;
        }
    }
    let bin_hz = sr as f64 / frame_size as f64;
    let num: f64 = accum.iter().enumerate().map(|(k, &m)| k as f64 * bin_hz * m).sum();
    let den: f64 = accum.iter().sum();
    if den < 1e-20 {
        f64::NAN
    } else {
        num / den
    }
}

/// Fraction of ACT_CHUNK_MS-sized chunks where max(|dl|,|dr|) exceeds
/// ACT_DBFS_THRESHOLD dBFS.
fn activation_fraction(dl: &[f32], dr: &[f32], sr: u32) -> f64 {
    let chunk_len = ((sr as f64) * ACT_CHUNK_MS / 1000.0).round().max(1.0) as usize;
    let n_chunks = dl.len() / chunk_len;
    if n_chunks == 0 {
        return f64::NAN;
    }
    let mut active = 0usize;
    for c in 0..n_chunks {
        let s = c * chunk_len;
        let e = s + chunk_len;
        let mut peak = 0.0_f32;
        for i in s..e {
            peak = peak.max(dl[i].abs()).max(dr[i].abs());
        }
        let dbfs = if peak > 0.0 { 20.0 * (peak as f64).log10() } else { f64::NEG_INFINITY };
        if dbfs > ACT_DBFS_THRESHOLD {
            active += 1;
        }
    }
    active as f64 / n_chunks as f64
}

/// Find the start sample of the CLIP_SECS window with the densest 5-9kHz
/// energy, searched on the UNPROCESSED mono downmix.
fn find_densest_sibilant_window(mono: &[f32], sr: u32) -> usize {
    let frames = stft_magnitudes(mono, FRAME_SIZE, HOP);
    if frames.is_empty() {
        return 0;
    }
    let bin_hz = sr as f64 / FRAME_SIZE as f64;
    let lo_bin = (BAND_LO_HZ / bin_hz).floor() as usize;
    let hi_bin = ((BAND_HI_HZ / bin_hz).ceil() as usize).min(frames[0].len() - 1);
    let frame_band_energy: Vec<f64> = frames
        .iter()
        .map(|f| f[lo_bin..=hi_bin].iter().map(|&m| m as f64).sum())
        .collect();

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

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    assert!(!paths.is_empty(), "usage: deess_threshold <file1> [file2] ...");
    std::fs::create_dir_all(AUDITION_DIR).expect("mkdir audition dir");

    let thresholds: [f32; 4] = [-30.0, -24.0, -18.0, -12.0];

    let mut audition_entries: Vec<String> = Vec::new();

    for path in &paths {
        println!("══════════════════════════════════════════════════════");
        println!("input: {path}");
        let (l0, r0, sr) = decode_stereo_native(path);
        let dur = l0.len() as f64 / sr as f64;
        println!("sr={sr} samples={} dur={:.2}s", l0.len(), dur);

        let stub: String = std::path::Path::new(path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
            .collect::<String>();
        let stub = stub.trim_matches('-').to_string();

        // Densest 5-9kHz 20s window, on the UNPROCESSED mono downmix.
        let mono: Vec<f32> = l0.iter().zip(r0.iter()).map(|(&a, &b)| (a + b) * 0.5).collect();
        let clip_start = find_densest_sibilant_window(&mono, sr);
        let clip_start_s = clip_start as f64 / sr as f64;
        println!("densest 5-9kHz 20s window starts at t={clip_start_s:.2}s");

        // Unprocessed clip.
        let (ul, ur) = slice_clip(&l0, &r0, clip_start, sr);
        let unproc_path = format!("{AUDITION_DIR}/{stub}__unprocessed.wav");
        WavWriter::write(&unproc_path, &ul, &ur, sr).expect("write unprocessed clip");
        audition_entries.push(format!(
            "| {stub} | ΑΝΕΠΕΞΕΡΓΑΣΤΟ | t={clip_start_s:.2}s..{:.2}s | `{unproc_path}` |",
            clip_start_s + CLIP_SECS
        ));

        println!();
        println!("{:>8}  {:>14}  {:>14}  {:>10}", "thr(dB)", "null_rms_db", "centroid(Hz)", "activation");
        for &thr in &thresholds {
            let mut l = l0.clone();
            let mut r = r0.clone();
            let mut node = DeEsserNode::new(sr);
            node.set_parameter("threshold_db", thr);
            node.set_parameter("frequency_hz", 6000.0);
            node.set_parameter("ratio", 4.0);
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

            let diff_mono: Vec<f32> = dl.iter().zip(dr.iter()).map(|(&a, &b)| (a + b) * 0.5).collect();
            let diff_frames = stft_magnitudes(&diff_mono, FRAME_SIZE, HOP);
            let centroid_hz = spectral_centroid_hz(&diff_frames, sr, FRAME_SIZE);

            let activation_frac = activation_fraction(&dl, &dr, sr);

            println!(
                "{:>8.1}  {:>14.4}  {:>14.1}  {:>9.2}%",
                thr, null_rms_db, centroid_hz, activation_frac * 100.0
            );

            let label = format!("neg{}", thr.abs() as i32);
            let (pl, pr) = slice_clip(&l, &r, clip_start, sr);
            let clip_path = format!("{AUDITION_DIR}/{stub}__thr_{label}.wav");
            WavWriter::write(&clip_path, &pl, &pr, sr).expect("write threshold clip");
            audition_entries.push(format!(
                "| {stub} | thr={thr:.1}dB | t={clip_start_s:.2}s..{:.2}s | `{clip_path}` |",
                clip_start_s + CLIP_SECS
            ));

        }
        println!();
    }

    // audition.md
    let mut md = String::new();
    md.push_str("# de-esser threshold audition clips\n\n");
    md.push_str("| αρχείο | παραλλαγή | εύρος | path |\n");
    md.push_str("|---|---|---|---|\n");
    for e in &audition_entries {
        md.push_str(e);
        md.push('\n');
    }
    std::fs::write(format!("{AUDITION_DIR}/audition.md"), md).expect("write audition.md");
    println!("audition.md γράφτηκε στο {AUDITION_DIR}/audition.md");
}
