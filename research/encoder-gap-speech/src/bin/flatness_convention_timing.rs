//! MEASURE 2026-09-17: κόστος χρόνου της σύμβασης Β (μία κλήση
//! spectral_flatness() ανά παράθυρο 5s) έναντι της Α (δέκα κλήσεις σε
//! υπο-τμήματα 0.5s) — ΜΟΝΟ ο υπολογισμός της επιπεδότητας, ΕΞΩ από
//! την παραγωγή (καμία αλλαγή στο trunk_pass.rs σε αυτό το task).
//! Τρία τρεξίματα ανά σύμβαση, ίδιο dump (dracula), όπως το
//! flatness_rule_timing.rs 16/09.
//! ΧΡΗΣΗ: cargo run --release --bin flatness_convention_timing -- <dump_path>
use sp314_dsp::analysis::spectral::spectral_flatness;
use std::time::Instant;

const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const SUBCHUNK_SECS: f32 = 0.5;
const DUMP_FRAME_BYTES: usize = 8;

fn read_dump_mono(path: &str) -> Vec<f32> {
    let buf = std::fs::read(path).unwrap_or_else(|e| panic!("read dump {path}: {e}"));
    let full_frames = buf.len() / DUMP_FRAME_BYTES;
    (0..full_frames)
        .map(|i| {
            let base = i * DUMP_FRAME_BYTES;
            let l = f32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]);
            let r = f32::from_le_bytes([buf[base + 4], buf[base + 5], buf[base + 6], buf[base + 7]]);
            (l + r) * 0.5
        })
        .collect()
}

fn median(xs: &[f32]) -> f32 {
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n == 0 { return f32::NAN; }
    if n % 2 == 0 { (v[n / 2 - 1] + v[n / 2]) / 2.0 } else { v[n / 2] }
}

fn window_flatness_a(mono_slice: &[f32], sr: u32) -> f32 {
    let sub_samples = (SUBCHUNK_SECS * sr as f32) as usize;
    let mut vals = Vec::new();
    let mut pos = 0;
    while pos + sub_samples <= mono_slice.len() {
        vals.push(spectral_flatness(&mono_slice[pos..pos + sub_samples]));
        pos += sub_samples;
    }
    median(&vals)
}

fn window_flatness_b(mono_slice: &[f32]) -> f32 {
    spectral_flatness(mono_slice)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dump_path = args.get(1).cloned().unwrap_or_else(|| "/tmp/before_dracula.raw".to_string());
    let sr = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;
    let mono = read_dump_mono(&dump_path);
    let win_samples = (WINDOW_SECS * sr as f32) as usize;
    let hop_samples = (HOP_SECS * sr as f32) as usize;

    println!("dump={dump_path}  δείγματα={}", mono.len());

    println!("--- ΣΥΜΒΑΣΗ Α (10×0.5s, διάμεσος) ---");
    let mut times_a = Vec::new();
    for run in 0..3 {
        let t0 = Instant::now();
        let mut start = 0usize;
        let mut n = 0usize;
        while start + win_samples <= mono.len() {
            let _ = window_flatness_a(&mono[start..start + win_samples], sr);
            start += hop_samples;
            n += 1;
        }
        let elapsed = t0.elapsed().as_secs_f64();
        times_a.push(elapsed);
        println!("  run {}: {:.3}s ({n} παράθυρα)", run + 1, elapsed);
    }
    println!("  μέσος όρος Α: {:.3}s", times_a.iter().sum::<f64>() / 3.0);

    println!("--- ΣΥΜΒΑΣΗ Β (1×5s, πλήρες) ---");
    let mut times_b = Vec::new();
    for run in 0..3 {
        let t0 = Instant::now();
        let mut start = 0usize;
        let mut n = 0usize;
        while start + win_samples <= mono.len() {
            let _ = window_flatness_b(&mono[start..start + win_samples]);
            start += hop_samples;
            n += 1;
        }
        let elapsed = t0.elapsed().as_secs_f64();
        times_b.push(elapsed);
        println!("  run {}: {:.3}s ({n} παράθυρα)", run + 1, elapsed);
    }
    println!("  μέσος όρος Β: {:.3}s", times_b.iter().sum::<f64>() / 3.0);
}
