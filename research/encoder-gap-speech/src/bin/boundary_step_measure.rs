//! Βοηθητικό: μετράει το «σκαλοπάτι» γύρω από ένα όριο μέσα σε ένα
//! rendered αρχείο (200ms RMS πριν/μετά σε dB, φάσμα οκτώ μπαντών
//! εκατέρωθεν, μέγιστη στιγμιαία διαφορά δείγματος στο όριο).
//! Φάσμα οκτώ μπαντών: ΙΔΙΕΣ BAND_EDGES + butter_hp2/butter_lp2
//! (4ης τάξης, 2×HP+2×LP) με sp314_dsp::analysis::pre_analysis, ίδια
//! μέθοδο με το production 8-band spectral profile — νέος κώδικας,
//! καμία εξάρτηση από το trunk_pass.rs. ΔΕΝ αγγίζει παραγωγή.
//! ΧΡΗΣΗ: cargo run --release --bin boundary_step_measure -- <audio_path> <boundary_sec>
use sp314_dsp::analysis::pre_analysis::{butter_hp2, butter_lp2, Biquad, BAND_EDGES};

const SR: f32 = 48000.0;
const WIN_MS: f32 = 200.0;

fn rms_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return f32::NEG_INFINITY;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt() as f32;
    20.0 * (rms.max(1e-10)).log10()
}

fn band_rms_db(samples: &[f32], lo: f32, hi: f32, nyq: f32) -> f32 {
    let lo = lo.max(1.0);
    let hi = hi.min(nyq - 1.0);
    if lo >= hi {
        return f32::NEG_INFINITY;
    }
    let mut hp1 = butter_hp2(lo, SR);
    let mut hp2 = butter_hp2(lo, SR);
    let mut lp1 = butter_lp2(hi, SR);
    let mut lp2 = butter_lp2(hi, SR);
    let filtered: Vec<f32> = samples
        .iter()
        .map(|&s| {
            let x = hp2.process(hp1.process(s));
            lp2.process(lp1.process(x))
        })
        .collect();
    rms_db(&filtered)
}

fn eight_band_db(samples: &[f32]) -> [f32; 8] {
    let nyq = SR / 2.0;
    let mut out = [0.0f32; 8];
    for i in 0..8 {
        out[i] = band_rms_db(samples, BAND_EDGES[i], BAND_EDGES[i + 1], nyq);
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let audio_path = &args[1];
    let boundary_sec: f32 = args[2].parse().unwrap();

    // .pcm ⇒ ήδη headerless interleaved f32 LE stereo (production mastered
    // dump convention) — διαβάζεται απευθείας. Οτιδήποτε άλλο ⇒ decode
    // μέσω της ΠΡΑΓΜΑΤΙΚΗΣ production decode συνάρτησης (wav/flac/mp3).
    let buf = if audio_path.ends_with(".pcm") {
        std::fs::read(audio_path).unwrap()
    } else {
        let dump = format!("/tmp/step_measure_{}.raw", std::process::id());
        m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(audio_path), &dump)
            .unwrap_or_else(|e| panic!("decode {audio_path}: {e}"));
        let b = std::fs::read(&dump).unwrap();
        let _ = std::fs::remove_file(&dump);
        b
    };
    let full_frames = buf.len() / 8;
    let mono: Vec<f32> = (0..full_frames)
        .map(|i| {
            let base = i * 8;
            let l = f32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]);
            let r = f32::from_le_bytes([buf[base + 4], buf[base + 5], buf[base + 6], buf[base + 7]]);
            (l + r) * 0.5
        })
        .collect();

    let boundary_frame = (boundary_sec * SR) as usize;
    let win_frames = (WIN_MS / 1000.0 * SR) as usize;
    let before_start = boundary_frame.saturating_sub(win_frames);
    let before = &mono[before_start..boundary_frame.min(mono.len())];
    let after_end = (boundary_frame + win_frames).min(mono.len());
    let after = &mono[boundary_frame.min(mono.len())..after_end];

    println!("=== όριο {:.3}s (frame {boundary_frame}), αρχείο={audio_path} ===", boundary_sec);
    println!("  RMS 200ms πριν : {:.2} dB", rms_db(before));
    println!("  RMS 200ms μετά : {:.2} dB", rms_db(after));
    println!("  RMS βήμα (μετά-πριν): {:+.2} dB", rms_db(after) - rms_db(before));

    let bands_before = eight_band_db(before);
    let bands_after = eight_band_db(after);
    for i in 0..8 {
        println!(
            "  band[{:>5.0}-{:>5.0}Hz]  πριν={:>7.2}dB  μετά={:>7.2}dB  Δ={:+.2}dB",
            BAND_EDGES[i], BAND_EDGES[i + 1], bands_before[i], bands_after[i], bands_after[i] - bands_before[i]
        );
    }

    // Μέγιστη στιγμιαία διαφορά δείγματος γύρω από το ίδιο το όριο (±20 δείγματα).
    let lo = boundary_frame.saturating_sub(20);
    let hi = (boundary_frame + 20).min(mono.len() - 1);
    let mut max_diff = 0.0f32;
    for i in lo..hi {
        let d = (mono[i + 1] - mono[i]).abs();
        if d > max_diff {
            max_diff = d;
        }
    }
    println!("  μέγιστη στιγμιαία |Δδείγματος| γύρω από το όριο (±20 δείγματα): {:.6}", max_diff);
}
