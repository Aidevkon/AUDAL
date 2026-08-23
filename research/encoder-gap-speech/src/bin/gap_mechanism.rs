//! F-077 ΓΥΡΟΣ 3 — ΜΗΧΑΝΙΣΜΟΣ: σταθερή στάθμη ή φασματική απώλεια;
//! ΜΕΤΡΗΣΗ, ΟΧΙ gate. Κανένα κατώφλι, καμία verdict λογική, κανένα
//! margin προτείνεται.
//!
//! ΤΟ ΕΥΡΗΜΑ ΠΟΥ ΤΟ ΓΕΝΝΑ: τέσσερα όργανα (unweighted RMS, K-weighted
//! gated LUFS, sample peak, oversampled true peak) έδωσαν όλα Δ ≈ −0.27
//! στην ομιλία (γύρος 1+2). Υπόθεση: ομοιόμορφος πολλαπλασιαστής
//! ×0.9698 (σταθερή στάθμη), ΟΧΙ φασματική απώλεια.
//!
//! ΔΟΚΙΜΗ Γ (πρώτη — προϋπόθεση για τις άλλες δύο): sample rate/channel
//! ταύτιση cert vs mp3.
//! ΔΟΚΙΜΗ Α (κύρια): 4 synthetic σήματα (1kHz/100Hz/8kHz sine στα
//! ΑΚΡΙΒΩΣ -20.00 dBFS πλάτος, + λευκός θόρυβος) μέσα από το ΙΔΙΟ
//! export_mp3_acx, μέτρηση 4 μεγεθών (RMS/peak/LUFS/TP) cert vs decoded.
//! ΔΟΚΙΜΗ Β: 10 από τα 60 αρχεία ομιλίας, +0.2655 dB κέρδος στο DECODED,
//! ξαναμέτρηση.
//!
//! ΠΡΟΒΛΕΨΕΙΣ (γραμμένες ΠΡΙΝ τρέξει, δεν αγγίζονται μετά):
//!   Α: σταθερή στάθμη ⇒ Δ όλων των synthetic σημάτων ≈ −0.27 (ίδιο με
//!      ομιλία)· φασματική απώλεια ⇒ το ΚΑΘΑΡΟ ημίτονο (τίποτα να
//!      πεταχτεί/masked) δίνει Δ ΠΟΛΥ μικρότερο.
//!   Β: σταθερή στάθμη ⇒ όλα τα Δ (και οι 4 μετρικές) καταρρέουν σε
//!      |Δ| < 0.05 μετά το +0.2655 dB compensation· υπόλοιπο σε
//!      οποιαδήποτε μετρική ⇒ εκεί ζει η πραγματική φασματική συνιστώσα.
//! Αν πέσουν έξω, η απόκλιση ΕΙΝΑΙ το εύρημα.

use encoder_gap_speech::{
    ffmpeg_astats, ffmpeg_ebur128_on_file, ffprobe_duration_secs, find_all_flacs,
    fisher_yates_shuffle, flac_to_48k_stereo_pcm, make_blob, measure_rms_peak,
    rebuild_pre_encode_mono, write_f32le, Lcg, MIN_DURATION_SECS, SAMPLE_TARGET, SEED,
};
use m0d::handlers::export::export_mp3_acx;
use std::path::PathBuf;

const WORK_DIR: &str = "/tmp/gap-mechanism-work";
const CSV_A: &str = "/tmp/gap-mechanism-a-results.csv";
const CSV_B: &str = "/tmp/gap-mechanism-b-results.csv";
const SR: u32 = 48000;

fn sine_stereo(freq_hz: f32, amplitude: f32, secs: f32, sr: u32) -> Vec<f32> {
    let n = (sr as f32 * secs) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let v = amplitude * (2.0 * std::f32::consts::PI * freq_hz * t).sin();
        out.push(v); // L
        out.push(v); // R (mono content duplicated to stereo, same as ffmpeg -ac 2 upmix on the real LibriSpeech mono source)
    }
    out
}

/// Uniform white noise, deterministic (same LCG as the rest of this
/// crate — no new dependency). target_rms_db is LINEAR RMS in dBFS;
/// uniform distribution RMS = amplitude/sqrt(3), so amplitude =
/// 10^(db/20) * sqrt(3). DECLARED: the sines specify exact PEAK
/// amplitude ("πλάτος"); noise has no single peak, so this test targets
/// RMS at the same -20.00 dBFS number for a comparable "loudness".
fn white_noise_stereo(target_rms_db: f32, secs: f32, sr: u32, seed: u64) -> Vec<f32> {
    let n = (sr as f32 * secs) as usize;
    let amp = 10f32.powf(target_rms_db / 20.0) * 3f32.sqrt();
    let mut rng = Lcg(seed);
    let mut out = Vec::with_capacity(n * 2);
    for _ in 0..n {
        let bits = rng.next_u64();
        let v = ((bits >> 40) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * amp;
        out.push(v);
        out.push(v);
    }
    out
}

fn write_stereo_f32le(interleaved: &[f32], path: &std::path::Path) -> Result<(), String> {
    write_f32le(interleaved, path)
}

struct FourMetrics {
    cert_rms: f64,
    cert_peak: f64,
    cert_lufs: f64,
    cert_tp: f64,
    decoded_rms: f64,
    decoded_peak: f64,
    decoded_lufs: f64,
    decoded_tp: f64,
}
impl FourMetrics {
    fn delta_rms(&self) -> f64 { self.decoded_rms - self.cert_rms }
    fn delta_peak(&self) -> f64 { self.decoded_peak - self.cert_peak }
    fn delta_lufs(&self) -> f64 { self.decoded_lufs - self.cert_lufs }
    fn delta_tp(&self) -> f64 { self.decoded_tp - self.cert_tp }
}

/// Runs stereo_48k_pcm through the REAL export_mp3_acx, then measures
/// all four metrics on both cert (reconstructed pre-encode buffer) and
/// decoded (the actual delivered mp3) sides. Returns (metrics, mp3_path
/// left on disk for the caller to ffprobe/inspect further, num_frames).
fn measure_all_four(id: &str, stereo_48k_planar: &[Vec<f32>; 2], mp3_path: &std::path::Path) -> Result<FourMetrics, String> {
    let num_frames = stereo_48k_planar[0].len();
    let pcm_path = PathBuf::from(WORK_DIR).join(format!("{id}_48k.pcm"));
    let interleaved: Vec<f32> = stereo_48k_planar[0]
        .iter()
        .zip(stereo_48k_planar[1].iter())
        .flat_map(|(&l, &r)| [l, r])
        .collect();
    write_stereo_f32le(&interleaved, &pcm_path)?;

    let blob = make_blob(id, pcm_path.clone(), num_frames);
    let outcome = export_mp3_acx(&blob, mp3_path)?;
    let cert_rms = outcome.report.rms_db as f64;
    let cert_peak = outcome.report.sample_peak_db as f64;

    let pre_encode_mono = rebuild_pre_encode_mono(stereo_48k_planar);
    let (cross_rms, cross_peak) = measure_rms_peak(&pre_encode_mono, 44100);
    if (cross_rms - cert_rms).abs() >= 0.01 || (cross_peak - cert_peak).abs() >= 0.01 {
        return Err(format!(
            "cross-check mismatch: reconstructed rms={cross_rms:.4}/peak={cross_peak:.4} vs real cert rms={cert_rms:.4}/peak={cert_peak:.4}"
        ));
    }
    let cert_pcm_path = PathBuf::from(WORK_DIR).join(format!("{id}_cert_44k1_mono.pcm"));
    write_f32le(&pre_encode_mono, &cert_pcm_path)?;
    let (cert_lufs, cert_tp) = ffmpeg_ebur128_on_file(&["-hide_banner", "-f", "f32le", "-ar", "44100", "-ac", "1"], &cert_pcm_path)?;

    let (decoded_peak, decoded_rms) = ffmpeg_astats(&[], mp3_path)?;
    let (decoded_lufs, decoded_tp) = ffmpeg_ebur128_on_file(&["-hide_banner"], mp3_path)?;

    let _ = std::fs::remove_file(&pcm_path);
    let _ = std::fs::remove_file(&cert_pcm_path);

    Ok(FourMetrics { cert_rms, cert_peak, cert_lufs, cert_tp, decoded_rms, decoded_peak, decoded_lufs, decoded_tp })
}

fn planar_from_interleaved_stereo(interleaved: &[f32]) -> [Vec<f32>; 2] {
    let half = interleaved.len() / 2;
    let mut planar = [Vec::with_capacity(half), Vec::with_capacity(half)];
    for chunk in interleaved.chunks_exact(2) {
        planar[0].push(chunk[0]);
        planar[1].push(chunk[1]);
    }
    planar
}

fn main() {
    std::fs::create_dir_all(WORK_DIR).expect("create work dir");
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("FATAL: ffmpeg not found on PATH");
        std::process::exit(1);
    }

    // ═══════════════════════════════════════════════════════════════
    // ΔΟΚΙΜΗ Γ — sample rate / channel count, cert buffer vs mp3
    // ═══════════════════════════════════════════════════════════════
    println!("=== ΔΟΚΙΜΗ Γ — sample rate / channels ===");
    println!("cert buffer (rebuild_pre_encode_mono output, βλ. export.rs:542-543 target_sr=44100 · export.rs:735-738 lame_set_num_channels(1)+MPEG_mode::MONO):");
    println!("  sample_rate = 44100 Hz (ρητά ορισμένο, target_sr στο export.rs)");
    println!("  channels    = 1 (mono, lame_set_num_channels(gfp, 1) + MPEG_mode::MONO)");

    // Generate one throwaway 1kHz tone to produce a real mp3 and ffprobe it.
    let gamma_tone = sine_stereo(1000.0, 0.1, 2.0, SR);
    let gamma_planar = planar_from_interleaved_stereo(&gamma_tone);
    let gamma_mp3 = PathBuf::from(WORK_DIR).join("gamma_check.mp3");
    match measure_all_four("gamma", &gamma_planar, &gamma_mp3) {
        Ok(_) => {
            let probe = std::process::Command::new("ffprobe")
                .args(["-v", "error", "-show_entries", "stream=sample_rate,channels,codec_name", "-of", "default=noprint_wrappers=1"])
                .arg(&gamma_mp3)
                .output();
            match probe {
                Ok(o) => {
                    let out = String::from_utf8_lossy(&o.stdout);
                    println!("ffprobe πάνω στο ΠΡΑΓΜΑΤΙΚΟ delivered mp3 (gamma_check.mp3):");
                    for line in out.lines() {
                        println!("  {line}");
                    }
                    let matches = out.contains("sample_rate=44100") && out.contains("channels=1");
                    println!("ΤΑΥΤΙΣΗ cert vs mp3 (44100 Hz, mono): {}", if matches { "ΝΑΙ" } else { "ΟΧΙ — STOP, δες παρακάτω" });
                    if !matches {
                        eprintln!("FATAL: το mp3 ΔΕΝ έχει sample_rate=44100/channels=1 — μετράμε δύο διαφορετικά σήματα. Οι Δοκιμές Α/Β ΔΕΝ είναι έγκυρες μέχρι να διορθωθεί αυτό.");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("FATAL: ffprobe failed on gamma_check.mp3: {e}");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("FATAL: could not produce gamma check mp3: {e}");
            std::process::exit(1);
        }
    }
    let _ = std::fs::remove_file(&gamma_mp3);

    // ═══════════════════════════════════════════════════════════════
    // ΔΟΚΙΜΗ Α — oracle synthetic signals
    // ═══════════════════════════════════════════════════════════════
    println!("\n=== ΔΟΚΙΜΗ Α — oracle σήματα με γνωστή απάντηση ===");
    let mut csv_a = std::fs::File::create(CSV_A).expect("create csv a");
    use std::io::Write as IoWrite;
    writeln!(csv_a, "signal,cert_rms,cert_peak,cert_lufs,cert_tp,decoded_rms,decoded_peak,decoded_lufs,decoded_tp,delta_rms,delta_peak,delta_lufs,delta_tp").unwrap();

    let signals: Vec<(&str, Vec<f32>)> = vec![
        ("sine_1kHz_-20dBFS", sine_stereo(1000.0, 10f32.powf(-20.0 / 20.0), 10.0, SR)),
        ("sine_100Hz_-20dBFS", sine_stereo(100.0, 10f32.powf(-20.0 / 20.0), 10.0, SR)),
        ("sine_8kHz_-20dBFS", sine_stereo(8000.0, 10f32.powf(-20.0 / 20.0), 10.0, SR)),
        ("white_noise_-20dBFS_RMS", white_noise_stereo(-20.0, 10.0, SR, SEED)),
    ];

    let mut a_results: Vec<(&str, FourMetrics)> = Vec::new();
    for (name, interleaved) in &signals {
        let planar = planar_from_interleaved_stereo(interleaved);
        let mp3_path = PathBuf::from(WORK_DIR).join(format!("a_{name}.mp3"));
        match measure_all_four(name, &planar, &mp3_path) {
            Ok(m) => {
                println!(
                    "[{name}] cert(rms={:.3} peak={:.3} lufs={:.3} tp={:.3}) decoded(rms={:.3} peak={:.3} lufs={:.3} tp={:.3}) Δrms={:+.3} Δpeak={:+.3} Δlufs={:+.3} Δtp={:+.3}",
                    m.cert_rms, m.cert_peak, m.cert_lufs, m.cert_tp,
                    m.decoded_rms, m.decoded_peak, m.decoded_lufs, m.decoded_tp,
                    m.delta_rms(), m.delta_peak(), m.delta_lufs(), m.delta_tp()
                );
                writeln!(
                    csv_a, "{name},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}",
                    m.cert_rms, m.cert_peak, m.cert_lufs, m.cert_tp,
                    m.decoded_rms, m.decoded_peak, m.decoded_lufs, m.decoded_tp,
                    m.delta_rms(), m.delta_peak(), m.delta_lufs(), m.delta_tp()
                ).unwrap();
                a_results.push((name, m));
            }
            Err(e) => {
                eprintln!("[{name}] FAILED: {e}");
                writeln!(csv_a, "{name},FAILED,{e}").unwrap();
            }
        }
        let _ = std::fs::remove_file(&mp3_path);
    }
    csv_a.flush().unwrap();

    println!("\n--- ΔΟΚΙΜΗ Α, πρόβλεψη ↔ μετρημένο ---");
    println!("(αναφορά: ομιλία Δrms≈-0.269 Δpeak≈-0.265 Δlufs≈-0.2655 Δtp≈-0.264, βλ. γύρος 1+2)");
    for (name, m) in &a_results {
        println!(
            "  {name}: Δrms={:+.3} Δpeak={:+.3} Δlufs={:+.3} Δtp={:+.3}",
            m.delta_rms(), m.delta_peak(), m.delta_lufs(), m.delta_tp()
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // ΔΟΚΙΜΗ Β — αντιστάθμιση +0.2655 dB στο decoded
    // ═══════════════════════════════════════════════════════════════
    println!("\n=== ΔΟΚΙΜΗ Β — αντιστάθμιση +0.2655 dB στο decoded (10 αρχεία ομιλίας) ===");
    const COMPENSATION_DB: f64 = 0.2655;

    let librispeech_root = PathBuf::from("/home/aidevcon/Downloads/DATASET/librispeech/LibriSpeech/dev-clean");
    let mut all_flacs = find_all_flacs(&librispeech_root);
    let mut rng = Lcg(SEED);
    fisher_yates_shuffle(&mut all_flacs, &mut rng);
    let mut selected: Vec<PathBuf> = Vec::new();
    for flac in &all_flacs {
        if selected.len() >= SAMPLE_TARGET {
            break;
        }
        if let Some(d) = ffprobe_duration_secs(flac) {
            if d >= MIN_DURATION_SECS {
                selected.push(flac.clone());
            }
        }
    }
    // ΙΔΙΟ seed/shuffle/φίλτρο με τους γύρους 1+2 — "10 από τα 60" =
    // τα ΠΡΩΤΑ 10 της ΙΔΙΑΣ καθορισμένης λίστας των 60 (αναπαραγώγιμο,
    // όχι νέα τυχαία επιλογή).
    let ten = &selected[..10.min(selected.len())];

    let mut csv_b = std::fs::File::create(CSV_B).expect("create csv b");
    writeln!(csv_b, "path,delta_rms_before,delta_peak_before,delta_lufs_before,delta_tp_before,delta_rms_after,delta_peak_after,delta_lufs_after,delta_tp_after").unwrap();

    let mut before: Vec<[f64; 4]> = Vec::new();
    let mut after: Vec<[f64; 4]> = Vec::new();

    for (i, flac) in ten.iter().enumerate() {
        let id = format!("b-{i:02}");
        let pcm_path = PathBuf::from(WORK_DIR).join(format!("{id}_48k.pcm"));
        let mp3_path = PathBuf::from(WORK_DIR).join(format!("{id}.mp3"));
        let gained_pcm_path = PathBuf::from(WORK_DIR).join(format!("{id}_gained_44k1_mono.pcm"));

        let result = (|| -> Result<([f64; 4], [f64; 4]), String> {
            flac_to_48k_stereo_pcm(flac, &pcm_path)?;
            let bytes = std::fs::read(&pcm_path).map_err(|e| e.to_string())?;
            let interleaved: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            let planar = planar_from_interleaved_stereo(&interleaved);
            let num_frames = planar[0].len();

            let blob = make_blob(&id, pcm_path.clone(), num_frames);
            let outcome = export_mp3_acx(&blob, &mp3_path)?;
            let cert_rms = outcome.report.rms_db as f64;
            let cert_peak = outcome.report.sample_peak_db as f64;

            let pre_encode_mono = rebuild_pre_encode_mono(&planar);
            let cert_pcm_path = PathBuf::from(WORK_DIR).join(format!("{id}_cert_44k1_mono.pcm"));
            write_f32le(&pre_encode_mono, &cert_pcm_path)?;
            let (cert_lufs, cert_tp) = ffmpeg_ebur128_on_file(&["-hide_banner", "-f", "f32le", "-ar", "44100", "-ac", "1"], &cert_pcm_path)?;

            // BEFORE compensation — decoded as-is.
            let (decoded_peak, decoded_rms) = ffmpeg_astats(&[], &mp3_path)?;
            let (decoded_lufs, decoded_tp) = ffmpeg_ebur128_on_file(&["-hide_banner"], &mp3_path)?;
            let before_deltas = [decoded_rms - cert_rms, decoded_peak - cert_peak, decoded_lufs - cert_lufs, decoded_tp - cert_tp];

            // AFTER compensation — decode mp3, apply +COMPENSATION_DB gain,
            // re-measure the GAINED signal at the SAME 44.1kHz mono format
            // as the cert side (apples to apples).
            let status = std::process::Command::new("ffmpeg")
                .args(["-y", "-v", "error", "-i"])
                .arg(&mp3_path)
                .args(["-af", &format!("volume={COMPENSATION_DB}dB"), "-ar", "44100", "-ac", "1", "-f", "f32le"])
                .arg(&gained_pcm_path)
                .status()
                .map_err(|e| format!("ffmpeg gain apply failed: {e}"))?;
            if !status.success() {
                return Err(format!("ffmpeg gain apply exited {status}"));
            }
            let (gained_peak, gained_rms) = ffmpeg_astats(&["-f", "f32le", "-ar", "44100", "-ac", "1"], &gained_pcm_path)?;
            let (gained_lufs, gained_tp) = ffmpeg_ebur128_on_file(&["-hide_banner", "-f", "f32le", "-ar", "44100", "-ac", "1"], &gained_pcm_path)?;
            let after_deltas = [gained_rms - cert_rms, gained_peak - cert_peak, gained_lufs - cert_lufs, gained_tp - cert_tp];

            let _ = std::fs::remove_file(&cert_pcm_path);
            Ok((before_deltas, after_deltas))
        })();

        match result {
            Ok((b, a)) => {
                println!(
                    "[{i:02}] {} BEFORE(Δrms={:+.3} Δpeak={:+.3} Δlufs={:+.3} Δtp={:+.3}) AFTER(Δrms={:+.3} Δpeak={:+.3} Δlufs={:+.3} Δtp={:+.3})",
                    flac.display(), b[0], b[1], b[2], b[3], a[0], a[1], a[2], a[3]
                );
                writeln!(csv_b, "{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}", flac.display(), b[0], b[1], b[2], b[3], a[0], a[1], a[2], a[3]).unwrap();
                before.push(b);
                after.push(a);
            }
            Err(e) => {
                eprintln!("[{i:02}] FAILED {}: {e}", flac.display());
                writeln!(csv_b, "{},FAILED,{e}", flac.display()).unwrap();
            }
        }

        let _ = std::fs::remove_file(&pcm_path);
        let _ = std::fs::remove_file(&mp3_path);
        let _ = std::fs::remove_file(&gained_pcm_path);
    }
    csv_b.flush().unwrap();

    println!("\n--- ΔΟΚΙΜΗ Β, |Δ| median πριν/μετά ανά μετρική (n={}) ---", before.len());
    let labels = ["RMS", "PEAK", "LUFS", "TP"];
    for (idx, label) in labels.iter().enumerate() {
        let mut b: Vec<f64> = before.iter().map(|x| x[idx].abs()).collect();
        let mut a: Vec<f64> = after.iter().map(|x| x[idx].abs()).collect();
        b.sort_by(|x, y| x.partial_cmp(y).unwrap());
        a.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let bmed = encoder_gap_speech::percentile(&b, 50.0);
        let amed = encoder_gap_speech::percentile(&a, 50.0);
        println!("  {label}: |Δ| median πριν={bmed:.4}  μετά={amed:.4}  -> {}", if amed < 0.05 { "ΚΑΤΕΡΡΕΥΣΕ (<0.05)" } else { "ΥΠΟΛΕΙΜΜΑ (>=0.05)" });
    }

    let _ = std::fs::remove_dir_all(WORK_DIR);
    println!("\nDONE");
}
