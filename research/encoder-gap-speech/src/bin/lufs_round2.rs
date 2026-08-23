//! F-077 ΓΥΡΟΣ 2 — LUFS/True-Peak χάσμα, ΙΔΙΟ σκάφος/seed/δείγμα με τον
//! γύρο 1 (main.rs, RMS/sample-peak). ΜΕΤΡΗΣΗ, όχι gate: κανένα κατώφλι,
//! καμία verdict λογική, κανένα margin.
//!
//! ΓΙΑΤΙ ΑΥΤΟ ΤΟ ΑΡΧΕΙΟ ΥΠΑΡΧΕΙ: streaming/podcast/broadcast δεν κρίνονται
//! με unweighted RMS + sample peak (τα ACX μεγέθη του γύρου 1) — κρίνονται
//! με Integrated LUFS (K-weighted, gated, BS.1770) και True Peak dBTP
//! (oversampled). Ο γύρος 1 δεν λέει τίποτα γι' αυτά.
//!
//! ΠΗΓΗ ΤΟΥ "CERT" (pre-encode) — ΕΥΡΗΜΑ ΤΟΥ ΒΗΜΑΤΟΣ 0:
//! export.rs χρησιμοποιεί ήδη TruePeakMeter (μόνο για peak-trim, δεν
//! επιστρέφεται) και ΚΑΝΕΝΑ integrated LUFS. ΚΑΙ ΤΑ ΔΥΟ ΑΠΟΝΤΑ ως
//! δημόσιο output του deliver path — βλ. .reports/2026-08-23-
//! encoder-gap-lufs.md §"ΒΗΜΑ 0" για το πλήρες grep.
//!
//! ΤΙ ΕΓΙΝΕ: `encoder_gap_speech::rebuild_pre_encode_mono()` ξαναχτίζει
//! το pre-LAME buffer καλώντας ΤΙΣ ΙΔΙΕΣ δημόσιες πρωτογενείς συναρτήσεις
//! που το export.rs καλεί εσωτερικά — δεν είναι νέα λογική, ίδια βήματα.
//! ΕΛΕΓΧΟΣ ΑΚΕΡΑΙΟΤΗΤΑΣ (ανά αρχείο): η buffer περνάει από το πραγματικό
//! `AcxCheckAnalyzer` και συγκρίνεται με το `outcome.report` του
//! πραγματικού `export_mp3_acx()` (ανοχή 0.01 dB).
//!
//! DECODED πλευρά: ffmpeg `ebur128=peak=true` — ΙΔΙΟ όργανο και για τις
//! δύο πλευρές.
//!
//! ΠΡΟΒΛΕΨΗ (MAESTRO, 2026-08-23, γραμμένη ΠΡΙΝ τρέξει — δεν αγγίζεται μετά):
//!   LUFS: |Δ| median < 0.10 LU (< 40% του 0.269 RMS χάσματος του γύρου 1)
//!   TRUE PEAK: |Δ| p95 >= 0.278 dB (>= sample-peak p95 του γύρου 1)
//! Αν πέσουν έξω, η απόκλιση ΕΙΝΑΙ το εύρημα.
//!
//! 2026-08-23 (γύρος 3 prep): reimplemented on top of src/lib.rs — ίδιος
//! αλγόριθμος, ίδιες σταθερές/συναρτήσεις, τώρα shared με τα άλλα
//! binaries αντί για copy-paste. Behavior unchanged (ίδιο CSV σχήμα,
//! ίδιες κλήσεις ffmpeg).

use encoder_gap_speech::{
    ffmpeg_ebur128_on_file, ffprobe_duration_secs, find_all_flacs, fisher_yates_shuffle,
    flac_to_48k_stereo_pcm, make_blob, measure_rms_peak, percentile, rebuild_pre_encode_mono,
    write_f32le, Lcg, MIN_DURATION_SECS, SAMPLE_TARGET, SEED,
};
use m0d::handlers::export::export_mp3_acx;
use std::io::Write as IoWrite;
use std::path::PathBuf;

const RESULTS_CSV: &str = "/tmp/encoder-gap-lufs-results.csv";
const ROUND1_CSV: &str = "/tmp/encoder-gap-speech-results.csv";

struct Measurement {
    path: PathBuf,
    input_duration_secs: f64,
    mp3_bytes: u64,
    cross_check_rms_diff: f64,
    cross_check_peak_diff: f64,
    cert_lufs: f64,
    cert_tp: f64,
    decoded_lufs: f64,
    decoded_tp: f64,
    delta_lufs: f64,
    delta_tp: f64,
}

fn main() {
    let started = std::time::Instant::now();

    let librispeech_root = PathBuf::from("/home/aidevcon/Downloads/DATASET/librispeech/LibriSpeech/dev-clean");
    if !librispeech_root.is_dir() {
        eprintln!("FATAL: {} not found", librispeech_root.display());
        std::process::exit(1);
    }
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("FATAL: ffmpeg not found on PATH");
        std::process::exit(1);
    }
    if std::process::Command::new("ffprobe").arg("-version").output().is_err() {
        eprintln!("FATAL: ffprobe not found on PATH");
        std::process::exit(1);
    }

    {
        let filters = std::process::Command::new("ffmpeg").args(["-hide_banner", "-filters"]).output();
        let has_ebur128 = filters.map(|o| String::from_utf8_lossy(&o.stdout).contains("ebur128")).unwrap_or(false);
        if !has_ebur128 {
            eprintln!("FATAL: this ffmpeg build does not list the ebur128 filter — cannot measure LUFS/True Peak. STOPPING, not substituting another filter.");
            std::process::exit(1);
        }
        let probe_wav = PathBuf::from("/tmp/encoder-gap-lufs-probe.wav");
        let gen = std::process::Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"])
            .arg(&probe_wav)
            .status();
        let probe_ok = gen.map(|s| s.success()).unwrap_or(false) && ffmpeg_ebur128_on_file(&[], &probe_wav).is_ok();
        let _ = std::fs::remove_file(&probe_wav);
        if !probe_ok {
            eprintln!("FATAL: ffmpeg 'ebur128=peak=true' probe failed on a synthetic tone — cannot proceed. STOPPING, not substituting another filter.");
            std::process::exit(1);
        }
        println!("ffmpeg ebur128=peak=true probe: OK");
    }

    let mut all_flacs = find_all_flacs(&librispeech_root);
    println!("corpus: {} flac files found under {}", all_flacs.len(), librispeech_root.display());

    let mut rng = Lcg(SEED);
    fisher_yates_shuffle(&mut all_flacs, &mut rng);

    let work_dir = PathBuf::from("/tmp/encoder-gap-lufs-work");
    let _ = std::fs::remove_dir_all(&work_dir);
    std::fs::create_dir_all(&work_dir).expect("create work dir");

    let mut selected: Vec<(PathBuf, f64)> = Vec::new();
    let mut scanned = 0usize;
    for flac in &all_flacs {
        if selected.len() >= SAMPLE_TARGET {
            break;
        }
        scanned += 1;
        if let Some(d) = ffprobe_duration_secs(flac) {
            if d >= MIN_DURATION_SECS {
                selected.push((flac.clone(), d));
            }
        }
    }
    println!("seed={SEED}  scanned={scanned}  selected={} — ΤΑΥΤΟΣΗΜΟ δείγμα με τον γύρο 1 (ίδιο seed/shuffle/φίλτρο)", selected.len());

    let mut measurements: Vec<Measurement> = Vec::new();
    let mut failures: Vec<(PathBuf, String)> = Vec::new();
    let mut cross_check_mismatches: Vec<(PathBuf, f64, f64)> = Vec::new();

    let mut csv = std::fs::File::create(RESULTS_CSV).expect("create results csv");
    writeln!(csv, "idx,path,status,input_duration_secs,mp3_bytes,cross_check_rms_diff,cross_check_peak_diff,cert_lufs,cert_tp,decoded_lufs,decoded_tp,delta_lufs,delta_tp,error").expect("write csv header");
    csv.flush().expect("flush csv header");
    println!("results file (append-as-we-go): {RESULTS_CSV}");

    let mut delta_groups: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    const CANNED_MAJORITY_FRACTION: f64 = 0.5;

    let total = selected.len();
    for (i, (flac, input_duration_secs)) in selected.iter().enumerate() {
        let id = format!("egl-{i:03}");
        let pcm_path = work_dir.join(format!("{id}_48k.pcm"));
        let mp3_path = work_dir.join(format!("{id}.mp3"));
        let cert_pcm_path = work_dir.join(format!("{id}_cert_44k1_mono.pcm"));

        let result = (|| -> Result<Measurement, String> {
            flac_to_48k_stereo_pcm(flac, &pcm_path)?;
            let bytes = std::fs::read(&pcm_path).map_err(|e| e.to_string())?;
            let samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            let half = samples.len() / 2;
            let mut planar = [Vec::with_capacity(half), Vec::with_capacity(half)];
            for chunk in samples.chunks_exact(2) {
                planar[0].push(chunk[0]);
                planar[1].push(chunk[1]);
            }
            let num_frames = planar[0].len();
            if num_frames == 0 {
                return Err("zero frames after resample".into());
            }

            let blob = make_blob(&id, pcm_path.clone(), num_frames);
            let outcome = export_mp3_acx(&blob, &mp3_path)?;

            let pre_encode_mono = rebuild_pre_encode_mono(&planar);

            let (cross_rms, cross_peak) = measure_rms_peak(&pre_encode_mono, 44100);
            let cross_check_rms_diff = cross_rms - outcome.report.rms_db as f64;
            let cross_check_peak_diff = cross_peak - outcome.report.sample_peak_db as f64;

            write_f32le(&pre_encode_mono, &cert_pcm_path)?;

            let (cert_lufs, cert_tp) = ffmpeg_ebur128_on_file(&["-hide_banner", "-f", "f32le", "-ar", "44100", "-ac", "1"], &cert_pcm_path)?;
            let (decoded_lufs, decoded_tp) = ffmpeg_ebur128_on_file(&["-hide_banner"], &mp3_path)?;

            let mp3_bytes = std::fs::metadata(&mp3_path).map_err(|e| e.to_string())?.len();
            if mp3_bytes == 0 {
                return Err("delivered mp3 is 0 bytes".into());
            }

            Ok(Measurement {
                path: flac.clone(),
                input_duration_secs: *input_duration_secs,
                mp3_bytes,
                cross_check_rms_diff,
                cross_check_peak_diff,
                cert_lufs,
                cert_tp,
                decoded_lufs,
                decoded_tp,
                delta_lufs: decoded_lufs - cert_lufs,
                delta_tp: decoded_tp - cert_tp,
            })
        })();

        match result {
            Ok(m) => {
                let cross_flag = if m.cross_check_rms_diff.abs() >= 0.01 || m.cross_check_peak_diff.abs() >= 0.01 { " ⚠CROSS-CHECK-MISMATCH" } else { "" };
                println!(
                    "[{}/{}] {} dur={:.2}s mp3={}B cross(rms_diff={:+.4} peak_diff={:+.4}){cross_flag} cert(LUFS={:.2} TP={:.2}) decoded(LUFS={:.2} TP={:.2}) Δlufs={:+.3} Δtp={:+.3}",
                    i + 1, total, flac.display(), m.input_duration_secs, m.mp3_bytes,
                    m.cross_check_rms_diff, m.cross_check_peak_diff,
                    m.cert_lufs, m.cert_tp, m.decoded_lufs, m.decoded_tp, m.delta_lufs, m.delta_tp
                );
                if !cross_flag.is_empty() {
                    cross_check_mismatches.push((m.path.clone(), m.cross_check_rms_diff, m.cross_check_peak_diff));
                }
                if m.mp3_bytes < 512 {
                    println!("  WARN: mp3 suspiciously small ({} bytes)", m.mp3_bytes);
                }
                let key = ((m.delta_lufs * 1000.0).round() as i64, (m.delta_tp * 1000.0).round() as i64);
                let group = delta_groups.entry(key).or_default();
                group.push(i);
                if group.len() >= 2 {
                    println!("  NOTE: Δ pair ({:+.3},{:+.3}) shared with {} other file(s) so far", m.delta_lufs, m.delta_tp, group.len() - 1);
                }
                writeln!(csv, "{},{},ok,{:.3},{},{:.4},{:.4},{:.3},{:.3},{:.3},{:.3},{:.4},{:.4},", i, m.path.display(), m.input_duration_secs, m.mp3_bytes, m.cross_check_rms_diff, m.cross_check_peak_diff, m.cert_lufs, m.cert_tp, m.decoded_lufs, m.decoded_tp, m.delta_lufs, m.delta_tp).expect("write csv row");
                csv.flush().expect("flush csv row");
                measurements.push(m);
            }
            Err(e) => {
                eprintln!("[{}/{}] FAILED {}: {e}", i + 1, total, flac.display());
                writeln!(csv, "{},{},failed,,,,,,,,,,,{}", i, flac.display(), e.replace(',', ";")).expect("write csv row");
                csv.flush().expect("flush csv row");
                failures.push((flac.clone(), e));
            }
        }

        let _ = std::fs::remove_file(&pcm_path);
        let _ = std::fs::remove_file(&mp3_path);
        let _ = std::fs::remove_file(&cert_pcm_path);
    }

    if let Some((key, group)) = delta_groups.iter().max_by_key(|(_, v)| v.len()) {
        let frac = group.len() as f64 / measurements.len().max(1) as f64;
        if frac >= CANNED_MAJORITY_FRACTION && measurements.len() >= 4 {
            eprintln!("FATAL: {}/{} measured files ({:.0}%) share the EXACT same (Δlufs,Δtp)={:?} pair — looks canned. Aborting before stats.", group.len(), measurements.len(), frac * 100.0, key);
            std::process::exit(1);
        }
    }

    let _ = std::fs::remove_dir_all(&work_dir);

    if measurements.is_empty() {
        eprintln!("FATAL: zero successful measurements out of {} attempted", selected.len());
        std::process::exit(1);
    }

    println!("\n=== SUCCESS: {}/{} files measured, {} failures ===", measurements.len(), selected.len(), failures.len());
    for (p, e) in &failures {
        println!("  FAILED: {} — {e}", p.display());
    }
    println!("\ncross-check integrity (reconstructed pre-encode buffer vs REAL export_mp3_acx report, tolerance 0.01 dB):");
    if cross_check_mismatches.is_empty() {
        println!("  ALL {} files matched within tolerance — the reconstruction IS the same instrument as the real cert.", measurements.len());
    } else {
        println!("  {} file(s) MISMATCHED — treat their LUFS/TP numbers as suspect:", cross_check_mismatches.len());
        for (p, rd, pd) in &cross_check_mismatches {
            println!("    {} rms_diff={:+.4} peak_diff={:+.4}", p.display(), rd, pd);
        }
    }

    let report = |label: &str, extract: fn(&Measurement) -> f64| {
        let mut vals: Vec<f64> = measurements.iter().map(extract).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = vals.len();
        let min = vals[0];
        let max = vals[n - 1];
        let p5 = percentile(&vals, 5.0);
        let median = percentile(&vals, 50.0);
        let p95 = percentile(&vals, 95.0);
        let pos = vals.iter().filter(|&&v| v > 0.0).count();
        let neg = vals.iter().filter(|&&v| v < 0.0).count();
        let zero = n - pos - neg;
        let mut abs_vals: Vec<f64> = vals.iter().map(|v| v.abs()).collect();
        abs_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let abs_median = percentile(&abs_vals, 50.0);
        let abs_p95 = percentile(&abs_vals, 95.0);
        println!("\n--- {label} (n={n}) ---");
        println!("  min={min:.3}  p5={p5:.3}  median={median:.3}  p95={p95:.3}  max={max:.3}");
        println!("  sign: positive={pos}  negative={neg}  zero={zero}");
        println!("  |Δ|: median={abs_median:.3}  p95={abs_p95:.3}");
        let outlier_threshold = 3.0 * abs_median;
        println!("  outlier threshold (3x |Δ| median) = {outlier_threshold:.3}");
        for m in &measurements {
            let v = extract(m).abs();
            if v > outlier_threshold && abs_median > 0.0 {
                println!("  OUTLIER: {} |Δ|={:.3} (> {:.3})", m.path.display(), v, outlier_threshold);
            }
        }
        (abs_median, abs_p95)
    };

    let (lufs_abs_median, _lufs_abs_p95) = report("LUFS Δ (decoded - cert)", |m| m.delta_lufs);
    let (_tp_abs_median, tp_abs_p95) = report("TRUE PEAK Δ (decoded - cert)", |m| m.delta_tp);

    println!("\n=== ΣΥΓΚΡΙΣΗ ΜΕ ΓΥΡΟ 1 ({ROUND1_CSV}, read-only) ===");
    match std::fs::read_to_string(ROUND1_CSV) {
        Ok(text) => {
            let mut r1_abs_rms: Vec<f64> = Vec::new();
            let mut r1_abs_peak: Vec<f64> = Vec::new();
            let mut r1_by_path: std::collections::HashMap<String, (f64, f64)> = std::collections::HashMap::new();
            for line in text.lines().skip(1) {
                let cols: Vec<&str> = line.split(',').collect();
                if cols.len() < 11 || cols[2] != "ok" {
                    continue;
                }
                let path = cols[1].to_string();
                let drms: f64 = cols[9].parse().unwrap_or(f64::NAN);
                let dpeak: f64 = cols[10].parse().unwrap_or(f64::NAN);
                if drms.is_nan() || dpeak.is_nan() {
                    continue;
                }
                r1_abs_rms.push(drms.abs());
                r1_abs_peak.push(dpeak.abs());
                r1_by_path.insert(path, (drms, dpeak));
            }
            r1_abs_rms.sort_by(|a, b| a.partial_cmp(b).unwrap());
            r1_abs_peak.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if !r1_abs_rms.is_empty() {
                let r1_rms_median = percentile(&r1_abs_rms, 50.0);
                let r1_peak_p95 = percentile(&r1_abs_peak, 95.0);
                println!("  round1 |Δrms| median={:.3}  round1 |Δpeak| p95={:.3}  (n={})", r1_rms_median, r1_peak_p95, r1_abs_rms.len());
                println!("  RMS vs LUFS:        |Δlufs| median = {lufs_abs_median:.4}  ({:.1}% of |Δrms| median {r1_rms_median:.3})", 100.0 * lufs_abs_median / r1_rms_median);
                println!("  sample-peak vs TP:  |Δtp| p95      = {tp_abs_p95:.4}  ({:.1}% of |Δpeak| p95 {r1_peak_p95:.3})", 100.0 * tp_abs_p95 / r1_peak_p95);
            } else {
                println!("  WARN: round 1 CSV had no 'ok' rows to compare against");
            }

            if let Some((r1_drms, r1_dpeak)) = r1_by_path.get("/home/aidevcon/Downloads/DATASET/librispeech/LibriSpeech/dev-clean/1988/147956/1988-147956-0028.flac") {
                if let Some(m) = measurements.iter().find(|m| m.path.to_string_lossy().contains("1988-147956-0028")) {
                    println!("\n  1988-147956-0028: round1 Δrms={r1_drms:+.4} Δpeak={r1_dpeak:+.4}  |  round2 Δlufs={:+.4} Δtp={:+.4}", m.delta_lufs, m.delta_tp);
                } else {
                    println!("\n  1988-147956-0028: present in round 1 but NOT in round 2's successful set — see failures list above");
                }
            } else {
                println!("\n  WARN: 1988-147956-0028 not found in round 1 CSV — cannot compare");
            }
        }
        Err(e) => println!("  WARN: could not read round 1 CSV ({e}) — skipping side-by-side comparison"),
    }

    println!("\n=== PREDICTION CHECK ===");
    println!("LUFS |Δ| median < 0.10 LU: predicted, measured {lufs_abs_median:.4} -> {}", if lufs_abs_median < 0.10 { "HOLDS" } else { "FALSIFIED" });
    println!("TRUE PEAK |Δ| p95 >= 0.278 dB: predicted, measured {tp_abs_p95:.4} -> {}", if tp_abs_p95 >= 0.278 { "HOLDS" } else { "FALSIFIED" });

    println!("\nelapsed: {:.1}s", started.elapsed().as_secs_f64());
}
