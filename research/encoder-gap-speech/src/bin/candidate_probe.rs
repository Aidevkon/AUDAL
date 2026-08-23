//! F-077/F-079 ΒΗΜΑ 0 — probe candidate material for noise_floor_db via
//! the REAL AcxCheckAnalyzer, at native sample rate. Read-only diagnostic,
//! no deliver path, no export_mp3_acx — just: decode -> feed -> report.
//! One-off tool for material qualification, not part of the measurement
//! pipeline proper.

use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use std::path::PathBuf;

fn decode_to_mono_f32(path: &str) -> Result<(Vec<f32>, u32), String> {
    let probe = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=sample_rate,channels", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    let out = String::from_utf8_lossy(&probe.stdout);
    let mut lines = out.lines();
    let sr: u32 = lines.next().ok_or("no sr")?.trim().parse().map_err(|_| "bad sr")?;

    let tmp = PathBuf::from(format!("/tmp/candidate_probe_{}.pcm", std::process::id()));
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(path)
        .args(["-ac", "1", "-f", "f32le"])
        .arg(&tmp)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("ffmpeg decode failed: {status}"));
    }
    let bytes = std::fs::read(&tmp).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    let samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    Ok((samples, sr))
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: candidate_probe <file1> [file2] ...");
        std::process::exit(1);
    }
    for path in &paths {
        match decode_to_mono_f32(path) {
            Ok((mono, sr)) => {
                let mut a = AcxCheckAnalyzer::new(sr);
                for chunk in mono.chunks(4096) {
                    a.feed_chunk(chunk);
                }
                let r = a.finish();
                let dur = mono.len() as f64 / sr as f64;
                println!(
                    "{path}: sr={sr} dur={dur:.2}s sample_peak_db={:.3} rms_db={:.3} noise_floor_db={:?} quietest_window_start_frame={:?}",
                    r.sample_peak_db, r.rms_db, r.noise_floor_db, r.quietest_window_start_frame
                );
                match r.noise_floor_db {
                    None => println!("  -> NONE (input < 1s ή αδύνατος υπολογισμός) — ΔΕΝ έχει αξιοποιήσιμες παύσεις"),
                    Some(nf) if (nf - r.rms_db).abs() < 0.5 => {
                        println!("  -> noise_floor ({nf:.3}) ΣΧΕΔΟΝ ΙΣΟ με το RMS ({:.3}) — δεν βρέθηκε ήσυχο παράθυρο, ΔΕΝ έχει πραγματικές παύσεις", r.rms_db)
                    }
                    Some(nf) => println!("  -> noise_floor ({nf:.3}) αισθητά κάτω από το RMS ({:.3}) — ΕΧΕΙ ήσυχα παράθυρα (πιθανές παύσεις)", r.rms_db),
                }
            }
            Err(e) => println!("{path}: FAILED — {e}"),
        }
    }
}
