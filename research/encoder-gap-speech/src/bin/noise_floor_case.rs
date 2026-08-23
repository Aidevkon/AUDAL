//! F-077/F-079 — single-file noise floor Δ measurement on REAL narration
//! with genuine room-tone pauses ("24 - The Wise In The Desert.flac",
//! user-supplied, qualified in .reports/2026-08-23-noise-floor-gap.md
//! addendum: 44.1kHz stereo, 154.88s, noise_floor -81.975dB vs RMS
//! -14.182dB — 67.8dB gap, confirmed non-digital-silence).
//!
//! ΜΕΤΡΗΣΗ, όχι gate. ΙΔΙΟ deliver path (export_mp3_acx) με όλους τους
//! προηγούμενους γύρους. Ένα αρχείο — ΔΗΛΩΜΕΝΟ ρητά ως ΠΡΩΤΗ ΜΕΤΡΗΣΗ,
//! όχι κατανομή, όχι margin.
//!
//! ΟΡΓΑΝΟ: για RMS/peak έχουμε εξωτερικό ένορκο (ffmpeg astats,
//! cross-check <0.05dB απαιτούμενο). Για noise floor ΔΕΝ υπάρχει
//! εξωτερικό ισοδύναμο (το astats δεν έχει windowed/HP-filtered
//! minimum) — άρα ΙΔΙΟ όργανο (ο δικός μας AcxCheckAnalyzer) και στις
//! δύο πλευρές. Αυτό είναι το σωστό όταν μετράς ΧΑΣΜΑ, όχι απόλυτη τιμή.
//!
//! ΠΡΟΒΛΕΨΗ (γραμμένη πριν τρέξει):
//!   RMS Δ ≈ -0.27 (όπως όλοι οι προηγούμενοι γύροι)
//!   peak Δ αρνητικό, |Δ| < 0.6
//!   NOISE FLOOR Δ: ΘΕΤΙΚΟ — ο κβαντιστής προσθέτει θόρυβο στις
//!     παύσεις εκεί που υπήρχε room tone. ΑΝΤΙΘΕΤΗ φορά από τα άλλα
//!     δύο, επικίνδυνη για το ACX (απαιτεί ≤ -60).

use encoder_gap_speech::{ffmpeg_astats, make_blob, rebuild_pre_encode_mono};
use m0d::handlers::export::export_mp3_acx;
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use std::path::PathBuf;

fn main() {
    let input = "/home/aidevcon/Downloads/DATASET/24 - The Wise In The Desert.flac";
    let work_dir = PathBuf::from("/tmp/noise-floor-case-work");
    std::fs::create_dir_all(&work_dir).expect("work dir");
    let pcm_path = work_dir.join("input_48k.pcm");
    let mp3_path = work_dir.join("output.mp3");

    // flac (44.1kHz stereo, native) -> 48kHz stereo f32 PCM, same
    // ffmpeg-based ingest as all previous rounds (round 1's
    // flac_to_48k_stereo_pcm — reused verbatim from lib.rs).
    encoder_gap_speech::flac_to_48k_stereo_pcm(std::path::Path::new(input), &pcm_path)
        .expect("flac -> 48k stereo pcm failed");

    let bytes = std::fs::read(&pcm_path).expect("read pcm");
    let samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let half = samples.len() / 2;
    let mut planar = [Vec::with_capacity(half), Vec::with_capacity(half)];
    for chunk in samples.chunks_exact(2) {
        planar[0].push(chunk[0]);
        planar[1].push(chunk[1]);
    }
    let num_frames = planar[0].len();
    println!("input: {input}");
    println!("48kHz stereo pcm: {num_frames} frames ({:.2}s)", num_frames as f64 / 48000.0);

    // 1. REAL deliver path.
    let blob = make_blob("noise-floor-case", pcm_path.clone(), num_frames);
    let outcome = export_mp3_acx(&blob, &mp3_path).expect("export_mp3_acx failed");
    let cert_rms = outcome.report.rms_db as f64;
    let cert_peak = outcome.report.sample_peak_db as f64;
    let cert_noise_floor = outcome.report.noise_floor_db.map(|v| v as f64);

    println!("\n=== CERT (pre-encode, from the REAL export_mp3_acx report) ===");
    println!("  rms_db          = {cert_rms:.4}");
    println!("  sample_peak_db  = {cert_peak:.4}");
    println!("  noise_floor_db  = {cert_noise_floor:?}");
    println!("  quietest_window_start_frame = {:?} ({:?}s @44.1kHz)",
        outcome.report.quietest_window_start_frame,
        outcome.report.quietest_window_start_frame.map(|f| f as f64 / 44100.0));

    // Cross-check: reconstruct the pre-encode buffer independently (same
    // method as round 2/3) and confirm the REAL report's numbers are
    // reproducible from the same public building blocks — integrity
    // check on the cert side before trusting the noise floor at all.
    let pre_encode_mono = rebuild_pre_encode_mono(&planar);
    let mut cross = AcxCheckAnalyzer::new(44100);
    for chunk in pre_encode_mono.chunks(4096) {
        cross.feed_chunk(chunk);
    }
    let cross_report = cross.finish();
    let cross_rms_diff = cross_report.rms_db as f64 - cert_rms;
    let cross_peak_diff = cross_report.sample_peak_db as f64 - cert_peak;
    let cross_nf_diff = match (cross_report.noise_floor_db, cert_noise_floor) {
        (Some(a), Some(b)) => Some(a as f64 - b),
        _ => None,
    };
    println!("\n=== CROSS-CHECK (reconstructed buffer vs REAL cert report) ===");
    println!("  rms_diff   = {cross_rms_diff:+.4}");
    println!("  peak_diff  = {cross_peak_diff:+.4}");
    println!("  noise_floor_diff = {cross_nf_diff:?}");
    if cross_rms_diff.abs() >= 0.01 || cross_peak_diff.abs() >= 0.01 {
        eprintln!("FATAL: cross-check mismatch >= 0.01dB — the reconstruction does not match the real cert. STOPPING per protocol.");
        std::process::exit(1);
    }

    // 3. Decode mp3, run OUR OWN AcxCheckAnalyzer (same instrument both
    // sides for noise floor — no external equivalent exists).
    let decode_pcm_path = work_dir.join("decoded_44k1_mono.pcm");
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(&mp3_path)
        .args(["-ar", "44100", "-ac", "1", "-f", "f32le"])
        .arg(&decode_pcm_path)
        .status()
        .expect("spawn ffmpeg decode");
    if !status.success() {
        eprintln!("FATAL: ffmpeg decode of mp3 failed: {status}");
        std::process::exit(1);
    }
    let decoded_bytes = std::fs::read(&decode_pcm_path).expect("read decoded pcm");
    let decoded_mono: Vec<f32> = decoded_bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();

    let mut decoded_analyzer = AcxCheckAnalyzer::new(44100);
    for chunk in decoded_mono.chunks(4096) {
        decoded_analyzer.feed_chunk(chunk);
    }
    let decoded_report = decoded_analyzer.finish();
    let decoded_rms = decoded_report.rms_db as f64;
    let decoded_peak = decoded_report.sample_peak_db as f64;
    let decoded_noise_floor = decoded_report.noise_floor_db.map(|v| v as f64);

    println!("\n=== DECODED (ffmpeg-decoded mp3, ΔΙΚΟΣ ΜΑΣ AcxCheckAnalyzer — ΙΔΙΟ όργανο και στις δύο πλευρές) ===");
    println!("  rms_db          = {decoded_rms:.4}");
    println!("  sample_peak_db  = {decoded_peak:.4}");
    println!("  noise_floor_db  = {decoded_noise_floor:?}");
    println!("  quietest_window_start_frame = {:?} ({:?}s)",
        decoded_report.quietest_window_start_frame,
        decoded_report.quietest_window_start_frame.map(|f| f as f64 / 44100.0));

    // ── One targeted question: is the cert's quietest window the SAME
    // position as the decoded's, or did the decoded find a DIFFERENT
    // spot? If different: how much of Δnoise_floor is "the same silence
    // got quieter" vs "a different silence was found"? Replicates the
    // EXACT noise-floor path (8th-order Butterworth HP @10Hz, cascaded
    // biquads with pole-angle Qs, then 500ms = 5×100ms sub-block mean
    // square) using the SAME public primitives acx_check.rs calls
    // internally (sp314_dsp::analysis::pre_analysis::{butter_hp2_q,
    // Biquad}) — not a new algorithm, the same one, evaluated at a
    // caller-chosen offset instead of the analyzer's own global minimum.
    if let (Some(cert_start), Some(decoded_start)) =
        (outcome.report.quietest_window_start_frame, decoded_report.quietest_window_start_frame)
    {
        println!("\n=== ΘΕΣΗ ΤΟΥ ΗΣΥΧΟΤΕΡΟΥ ΠΑΡΑΘΥΡΟΥ: cert vs decoded ===");
        println!("  cert    quietest_window_start_frame = {cert_start} ({:.3}s)", cert_start as f64 / 44100.0);
        println!("  decoded quietest_window_start_frame = {decoded_start} ({:.3}s)", decoded_start as f64 / 44100.0);
        if cert_start == decoded_start {
            println!("  -> ΙΔΙΑ ΘΕΣΗ. Ολόκληρο το Δnoise_floor είναι 'η ίδια σιωπή έγινε πιο σιωπηλή'.");
        } else {
            use sp314_dsp::analysis::pre_analysis::{butter_hp2_q, Biquad};
            const HP_CUTOFF_HZ: f32 = 10.0;
            const SR: f32 = 44100.0;
            let mut hp: [Biquad; 4] = core::array::from_fn(|k| {
                let theta = (2 * k + 1) as f32 * core::f32::consts::PI / 16.0;
                let q = 1.0 / (2.0 * (theta.cos()));
                butter_hp2_q(HP_CUTOFF_HZ, SR, q)
            });
            // Filter the WHOLE decoded buffer through the cascade (state
            // must run from sample 0 to be identical to what the real
            // streaming analyzer saw at the cert's window offset).
            let mut filtered = Vec::with_capacity(decoded_mono.len());
            for &s in &decoded_mono {
                let mut y = s;
                for section in hp.iter_mut() {
                    y = section.process(y);
                }
                filtered.push(y);
            }
            let window_len = (0.5 * SR) as usize; // 500ms = 5x100ms sub-blocks
            let start = cert_start as usize;
            let end = (start + window_len).min(filtered.len());
            let window = &filtered[start..end];
            let mean_sq = window.iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>() / window.len() as f64;
            let rms = mean_sq.sqrt();
            let db = if rms < 1e-10 { -144.0 } else { 20.0 * rms.log10() };
            println!("  -> ΑΛΛΗ ΘΕΣΗ (Δ={} samples = {:.3}s).", decoded_start as i64 - cert_start as i64, (decoded_start as f64 - cert_start as f64) / 44100.0);
            println!("  RMS του decoded ΣΤΗ ΘΕΣΗ ΤΟΥ CERT ({:.3}s), ίδιο HP8+500ms filter: {db:.4} dB", cert_start as f64 / 44100.0);
            let same_spot_delta = db - cert_noise_floor.unwrap();
            let different_spot_delta = decoded_noise_floor.unwrap() - db;
            println!("  ΑΠΟΣΥΝΘΕΣΗ του Δnoise_floor={:.4}:", decoded_noise_floor.unwrap() - cert_noise_floor.unwrap());
            println!("    'η ίδια σιωπή έγινε πιο σιωπηλή' (decoded@cert_pos - cert)      = {same_spot_delta:+.4}");
            println!("    'βρέθηκε άλλη, ακόμα πιο ήσυχη θέση' (decoded_global - decoded@cert_pos) = {different_spot_delta:+.4}");
        }
    }

    // Cross-validation: ffmpeg astats (external juror) on the SAME
    // decoded mp3, for RMS/peak ONLY (no noise-floor equivalent exists).
    // MUST agree with our own analyzer within 0.05dB, or STOP.
    let (astats_peak, astats_rms) = ffmpeg_astats(&[], &mp3_path).expect("ffmpeg astats failed");
    println!("\n=== ΔΙΑΣΤΑΥΡΩΣΗ (ffmpeg astats, εξωτερικός ένορκος, RMS/peak ΜΟΝΟ) ===");
    println!("  astats rms_db  = {astats_rms:.4}   (δικό μας: {decoded_rms:.4}, Δ={:.4})", astats_rms - decoded_rms);
    println!("  astats peak_db = {astats_peak:.4}   (δικό μας: {decoded_peak:.4}, Δ={:.4})", astats_peak - decoded_peak);
    let rms_agree = (astats_rms - decoded_rms).abs() < 0.05;
    let peak_agree = (astats_peak - decoded_peak).abs() < 0.05;
    if !rms_agree || !peak_agree {
        eprintln!("\nFATAL: astats vs δικός μας αναλυτής ΔΕΝ συμφωνούν εντός 0.05dB.");
        eprintln!("  rms agree={rms_agree} (|Δ|={:.4})  peak agree={peak_agree} (|Δ|={:.4})", (astats_rms-decoded_rms).abs(), (astats_peak-decoded_peak).abs());
        eprintln!("Το όργανο έχει πρόβλημα. ΟΛΑ τα παραπάνω είναι ύποπτα. STOPPING per protocol.");
        std::process::exit(1);
    }
    println!("  ΣΥΜΦΩΝΙΑ: rms |Δ|<0.05 = {rms_agree}, peak |Δ|<0.05 = {peak_agree} — το όργανο επικυρώθηκε.");

    // 4. Δ = decoded - pre-encode, signed, for all three.
    let delta_rms = decoded_rms - cert_rms;
    let delta_peak = decoded_peak - cert_peak;
    let delta_noise_floor = match (decoded_noise_floor, cert_noise_floor) {
        (Some(d), Some(c)) => Some(d - c),
        _ => None,
    };

    println!("\n=== Δ = decoded - pre-encode (ΜΕ ΠΡΟΣΗΜΟ) ===");
    println!("  Δrms         = {delta_rms:+.4}");
    println!("  Δpeak        = {delta_peak:+.4}");
    println!("  Δnoise_floor = {delta_noise_floor:?}");

    println!("\n=== ΠΡΟΒΛΕΨΗ ↔ ΜΕΤΡΗΜΕΝΟ ===");
    println!("RMS Δ ≈ -0.27: measured {delta_rms:+.4} -> {}", if (delta_rms - (-0.27)).abs() < 0.15 { "ΣΥΝΕΠΕΣ" } else { "ΑΠΟΚΛΙΝΕΙ" });
    println!("peak Δ αρνητικό, |Δ|<0.6: measured {delta_peak:+.4} -> {}", if delta_peak < 0.0 && delta_peak.abs() < 0.6 { "HOLDS" } else { "FALSIFIED" });
    match delta_noise_floor {
        Some(d) if d > 0.0 => println!("noise floor Δ ΘΕΤΙΚΟ: measured {d:+.4} -> HOLDS"),
        Some(d) => println!("noise floor Δ ΘΕΤΙΚΟ: measured {d:+.4} -> FALSIFIED (η πρόβλεψη έπεσε έξω, δεν εξομαλύνεται)"),
        None => println!("noise floor Δ: ΔΕΝ ΥΠΟΛΟΓΙΣΤΗΚΕ (None σε μία από τις δύο πλευρές)"),
    }

    let _ = std::fs::remove_dir_all(&work_dir);
}
