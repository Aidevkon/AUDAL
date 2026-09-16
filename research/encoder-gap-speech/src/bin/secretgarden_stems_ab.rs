//! ΜΕΤΡΗΣΗ 2026-09-16: ζεύγος ακρόασης Α/Β για το F-102 (eb4bc5d) —
//! τα stems γεμίζονταν στον φυσικό ρυθμό (44100) και διαβάζονταν στον
//! ρυθμό της ροής (48000). ΠΛΗΡΗΣ αλυσίδα, πραγματικό executor.rs
//! (m0d::agents::executor::execute_streaming_plan), όχι δικό μας
//! στήσιμο του γράφου.
//!
//! Α = παλιά συνταγή (LazyAudioReader στο πρωτότυπο αρχείο, φυσικός
//!     ρυθμός) — παράγεται με προσωρινή παράκαμψη στο ΙΔΙΟ το
//!     executor.rs, σφραγίδα f2a9f86-style, επαναφορά αμέσως μετά.
//! Β = σημερινός κώδικας (DumpSeekProvider στο standardized dump) —
//!     ανέγγιχτο.
//!
//! ΧΡΗΣΗ: cargo run --release --bin secretgarden_stems_ab -- A|B

use rustfft::{num_complex::Complex, FftPlanner};

const SOURCE_MP3: &str = "/home/aidevcon/Downloads/DATASET/librivox-hq/secretgarden_01_burnett.mp3";
const OUT_DIR: &str = "/tmp/secretgarden-stems-audition";
const LTASS_CFS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];
const BAND_EDGES: [f32; 9] = [
    20.0, 80.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 20000.0,
];

fn read_mono_16bit_wav(path: &str) -> (Vec<f32>, u32) {
    let mut reader = hound::WavReader::open(path).expect("open wav");
    let spec = reader.spec();
    let sr = spec.sample_rate;
    let ch = spec.channels as usize;
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect();
    let mono: Vec<f32> = if ch == 1 {
        samples
    } else {
        samples
            .chunks_exact(ch)
            .map(|f| f.iter().sum::<f32>() / ch as f32)
            .collect()
    };
    (mono, sr)
}

fn welch_psd(x: &[f32], nfft: usize) -> Vec<f64> {
    let hop = nfft / 2;
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(nfft);
    let hann: Vec<f64> = (0..nfft)
        .map(|i| {
            let w = 2.0 * std::f64::consts::PI * i as f64 / nfft as f64;
            0.5 - 0.5 * w.cos()
        })
        .collect();
    let bins = nfft / 2 + 1;
    let mut acc = vec![0.0_f64; bins];
    let mut segments = 0usize;
    let mut pos = 0usize;
    while pos + nfft <= x.len() {
        let mut buf: Vec<Complex<f64>> = (0..nfft)
            .map(|i| Complex::new(x[pos + i] as f64 * hann[i], 0.0))
            .collect();
        fft.process(&mut buf);
        for (k, a) in acc.iter_mut().enumerate() {
            *a += buf[k].norm_sqr();
        }
        segments += 1;
        pos += hop;
    }
    let inv = 1.0 / segments.max(1) as f64;
    acc.iter().map(|v| v * inv).collect()
}

/// Ολοκλήρωμα ισχύος στις BAND_EDGES, σε dB (10·log10 της ισχύος —
/// ΟΧΙ σχετικό με flat αναφορά αυτή τη φορά, απόλυτο dB της ίδιας της
/// PSD, ώστε τα τρία αρχεία να συγκρίνονται απευθείας μεταξύ τους).
fn band_powers_db(mono: &[f32], sr: u32) -> [f64; 8] {
    let nfft = 4096usize.min(1 << (mono.len().max(2) as f64).log2().floor() as u32);
    let nfft = nfft.max(256);
    let psd = welch_psd(mono, nfft);
    let bin_hz = sr as f64 / nfft as f64;
    std::array::from_fn(|k| {
        let lo = BAND_EDGES[k] as f64;
        let hi = BAND_EDGES[k + 1] as f64;
        let mut sum = 0.0;
        for (i, p) in psd.iter().enumerate() {
            let f = i as f64 * bin_hz;
            if f >= lo && f < hi {
                sum += p;
            }
        }
        10.0 * sum.max(1e-20).log10()
    })
}

fn spectrum(path: &str) {
    let (mono, sr) = read_mono_16bit_wav(path);
    let bands = band_powers_db(&mono, sr);
    println!("ΦΑΣΜΑ {path} (sr={sr}, {} δείγματα):", mono.len());
    for k in 0..8 {
        println!("  band {k}  {:>7.1} Hz  {:>8.2} dB", LTASS_CFS[k], bands[k]);
    }
}

fn compare(source_path: &str, other_paths: &[String]) {
    let (src_mono, src_sr) = read_mono_16bit_wav(source_path);
    let src_bands = band_powers_db(&src_mono, src_sr);
    println!("ΑΝΑΦΟΡΑ (πρωτότυπο) {source_path}:");
    for k in 0..8 {
        println!("  band {k}  {:>7.1} Hz  {:>8.2} dB", LTASS_CFS[k], src_bands[k]);
    }
    for p in other_paths {
        let (mono, sr) = read_mono_16bit_wav(p);
        let bands = band_powers_db(&mono, sr);
        let mut sum_abs = 0.0_f64;
        println!("\n{p}:");
        for k in 0..8 {
            let diff = bands[k] - src_bands[k];
            sum_abs += diff.abs();
            println!(
                "  band {k}  {:>7.1} Hz  {:>8.2} dB  (Δ {:>+6.2} dB)",
                LTASS_CFS[k], bands[k], diff
            );
        }
        println!("  Σ|διαφορά| από το πρωτότυπο: {sum_abs:.2} dB");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("spectrum") {
        spectrum(args.get(2).expect("wav path"));
        return;
    }
    if args.get(1).map(|s| s.as_str()) == Some("compare") {
        let source = args.get(2).expect("source wav path");
        let others: Vec<String> = args[3..].to_vec();
        compare(source, &others);
        return;
    }

    let label = args.get(1).map(|s| s.as_str()).unwrap_or("B");

    std::fs::create_dir_all(OUT_DIR).ok();

    let plan = m0d::agents::operator::StreamingPlan {
        audio_path: SOURCE_MP3.to_string(),
        preset_id: "podcast".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: format!("secretgarden-stems-ab-{label}"),
    };

    let (output, _blob) =
        m0d::agents::executor::execute_streaming_plan(&plan, None).expect("execute_streaming_plan");

    let src_path = m0d::spool::spool_dir().join(format!("m0d-v3-streaming-{}.wav", output.blob_id));
    let dst_path = format!("{OUT_DIR}/{label}_full.wav");
    std::fs::copy(&src_path, &dst_path).expect("copy rendered output");
    println!(
        "Ρendered: {} -> {} (blob_id={})",
        src_path.display(),
        dst_path,
        output.blob_id
    );

    // Απόσπασμα, ΤΜΗΜΑ 18 (147.00s-162.00s, Speech, leaning=0.6663
    // conf=0.3290) — 3.0s μέσα στο τμήμα ως 11.0s (8s), εντός του
    // 91.875% (13.78s) που η παλιά συνταγή προλαβαίνει να διαβάσει.
    let excerpt_start = 147.0 + 3.0; // 150.0s απόλυτα
    let excerpt_len = 8.0;
    let excerpt_path = format!("{OUT_DIR}/{label}_excerpt_150-158s.wav");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{excerpt_start:.3}"),
            "-i",
            &dst_path,
            "-t",
            &format!("{excerpt_len}"),
            "-acodec",
            "pcm_s16le",
            &excerpt_path,
        ])
        .status()
        .expect("ffmpeg spawn");
    assert!(status.success(), "ffmpeg excerpt failed");
    println!("Απόσπασμα: {excerpt_path}");
}
