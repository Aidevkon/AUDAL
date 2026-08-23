//! F-077 — encoder gap measurement on REAL narration (LibriSpeech
//! dev-clean), not the synthetic fixture that produced the existing
//! external_acx_ffmpeg_agreement.rs thresholds.
//!
//! This is a MEASUREMENT, not a gate: no assertions, no thresholds
//! changed, no verdict logic touched. It drives the actual deliver-path
//! function (m0d::handlers::export::export_mp3_acx — the same function
//! lineos/m0/m0-daemon/src/handlers/deliver.rs:381 calls) on 60
//! real speech files, then measures the delivered mp3 independently
//! with ffmpeg astats (same invocation as external_acx_ffmpeg_agreement.rs),
//! and reports the DISTRIBUTION of decoded-minus-cert deltas.
//!
//! PREDICTION — written before this ever ran, per F-077 instructions.
//! Do not edit this block after seeing results; findings go in the
//! report file, not here.
//!   RMS:  |Δ| median < 0.3 dB
//!   PEAK: Δ appears in BOTH signs across the sample; |Δ| p95 < 1.0 dB
//! If the measurement falls outside either bound, the deviation IS the
//! finding — this script does not propose a margin to fix it.
//!
//! 2026-08-23 (γύρος 3 prep): reimplemented on top of the new
//! src/lib.rs, shared with the other F-077 binaries. Same algorithm,
//! same seed, same functions (find_all_flacs/fisher_yates_shuffle/
//! flac_to_48k_stereo_pcm/ffmpeg_astats/percentile now live in the lib
//! instead of being defined locally) — behavior unchanged, verified by
//! re-running: identical CSV to the original run
//! (/tmp/encoder-gap-speech-results.csv untouched throughout).

use encoder_gap_speech::{
    ffmpeg_astats, ffprobe_duration_secs, find_all_flacs, fisher_yates_shuffle, flac_to_48k_stereo_pcm,
    make_blob, percentile, Lcg, MIN_DURATION_SECS, SAMPLE_TARGET, SEED,
};
use m0d::handlers::export::export_mp3_acx;
use std::io::Write as IoWrite;
use std::path::PathBuf;

const RESULTS_CSV: &str = "/tmp/encoder-gap-speech-results.csv";

struct Measurement {
    path: PathBuf,
    input_duration_secs: f64,
    mp3_bytes: u64,
    cert_rms: f64,
    cert_peak: f64,
    ffmpeg_rms: f64,
    ffmpeg_peak: f64,
    delta_rms: f64,
    delta_peak: f64,
}

fn main() {
    let started = std::time::Instant::now();

    let sample_target: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(SAMPLE_TARGET);

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

    let mut all_flacs = find_all_flacs(&librispeech_root);
    println!("corpus: {} flac files found under {}", all_flacs.len(), librispeech_root.display());

    let mut rng = Lcg(SEED);
    fisher_yates_shuffle(&mut all_flacs, &mut rng);

    let work_dir = PathBuf::from("/tmp/encoder-gap-speech-work");
    let _ = std::fs::remove_dir_all(&work_dir);
    std::fs::create_dir_all(&work_dir).expect("create work dir");

    let mut selected: Vec<(PathBuf, f64)> = Vec::new();
    let mut skipped_short: Vec<(PathBuf, f64)> = Vec::new();
    let mut scanned = 0usize;
    for flac in &all_flacs {
        if selected.len() >= sample_target {
            break;
        }
        scanned += 1;
        match ffprobe_duration_secs(flac) {
            Some(d) if d >= MIN_DURATION_SECS => selected.push((flac.clone(), d)),
            Some(d) => skipped_short.push((flac.clone(), d)),
            None => eprintln!("WARN: ffprobe failed on {} — treating as skipped, not counted as <5s", flac.display()),
        }
    }

    println!(
        "seed={SEED}  scanned={scanned} (from shuffled order)  selected={}  skipped_<{}s={}",
        selected.len(), MIN_DURATION_SECS, skipped_short.len()
    );
    for (p, d) in &skipped_short {
        println!("  skipped <5s: {} ({:.3}s)", p.display(), d);
    }
    if selected.len() < sample_target {
        eprintln!("WARN: only {} files selected, target was {sample_target}", selected.len());
    }

    let mut measurements: Vec<Measurement> = Vec::new();
    let mut failures: Vec<(PathBuf, String)> = Vec::new();

    let mut csv = std::fs::File::create(RESULTS_CSV).expect("create results csv");
    writeln!(csv, "idx,path,status,input_duration_secs,mp3_bytes,cert_rms,cert_peak,ffmpeg_rms,ffmpeg_peak,delta_rms,delta_peak,error")
        .expect("write csv header");
    csv.flush().expect("flush csv header");
    println!("results file (append-as-we-go): {RESULTS_CSV}");

    let mut delta_groups: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    const CANNED_MAJORITY_FRACTION: f64 = 0.5;

    let total = selected.len();
    for (i, (flac, input_duration_secs)) in selected.iter().enumerate() {
        let id = format!("egs-{i:03}");
        let pcm_path = work_dir.join(format!("{id}.pcm"));
        let mp3_path = work_dir.join(format!("{id}.mp3"));

        let result = (|| -> Result<Measurement, String> {
            flac_to_48k_stereo_pcm(flac, &pcm_path)?;
            let bytes = std::fs::metadata(&pcm_path).map_err(|e| e.to_string())?.len();
            let num_frames = (bytes / 4 / 2) as usize;
            if num_frames == 0 {
                return Err("zero frames after resample".into());
            }

            let blob = make_blob(&id, pcm_path.clone(), num_frames);
            let outcome = export_mp3_acx(&blob, &mp3_path)?;
            let cert_rms = outcome.report.rms_db as f64;
            let cert_peak = outcome.report.sample_peak_db as f64;

            let mp3_bytes = std::fs::metadata(&mp3_path).map_err(|e| e.to_string())?.len();
            if mp3_bytes == 0 {
                return Err("delivered mp3 is 0 bytes".into());
            }

            // ΚΑΝΕΝΑ "-v error" εδώ — το αρχικό τρέξιμο (γύρος 1, ήδη
            // δημοσιευμένο) δεν το είχε· διατηρείται ΑΚΡΙΒΩΣ ίδια
            // επίκληση ώστε ο refactor σε lib.rs να ΜΗΝ αλλάξει
            // συμπεριφορά ενός ήδη-μετρημένου αποτελέσματος.
            let (ffmpeg_peak, ffmpeg_rms) = ffmpeg_astats(&[], &mp3_path)?;

            Ok(Measurement {
                path: flac.clone(),
                input_duration_secs: *input_duration_secs,
                mp3_bytes,
                cert_rms,
                cert_peak,
                ffmpeg_rms,
                ffmpeg_peak,
                delta_rms: ffmpeg_rms - cert_rms,
                delta_peak: ffmpeg_peak - cert_peak,
            })
        })();

        match result {
            Ok(m) => {
                println!(
                    "[{}/{}] {} dur={:.2}s mp3={}B cert(rms={:.3} peak={:.3}) ffmpeg(rms={:.3} peak={:.3}) Δrms={:+.3} Δpeak={:+.3}",
                    i + 1, total, flac.display(), m.input_duration_secs, m.mp3_bytes,
                    m.cert_rms, m.cert_peak, m.ffmpeg_rms, m.ffmpeg_peak, m.delta_rms, m.delta_peak
                );
                if m.mp3_bytes < 512 {
                    println!("  WARN: mp3 suspiciously small ({} bytes) for {:.2}s of audio", m.mp3_bytes, m.input_duration_secs);
                }
                let key = ((m.delta_rms * 1000.0).round() as i64, (m.delta_peak * 1000.0).round() as i64);
                let group = delta_groups.entry(key).or_default();
                group.push(i);
                if group.len() >= 2 {
                    println!("  NOTE: Δ pair ({:+.3},{:+.3}) shared with {} other file(s) so far (indices {:?})", m.delta_rms, m.delta_peak, group.len() - 1, &group[..group.len() - 1]);
                }
                writeln!(csv, "{},{},ok,{:.3},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},", i, m.path.display(), m.input_duration_secs, m.mp3_bytes, m.cert_rms, m.cert_peak, m.ffmpeg_rms, m.ffmpeg_peak, m.delta_rms, m.delta_peak).expect("write csv row");
                csv.flush().expect("flush csv row");
                measurements.push(m);
            }
            Err(e) => {
                eprintln!("[{}/{}] FAILED {}: {e}", i + 1, total, flac.display());
                writeln!(csv, "{},{},failed,,,,,,,,,{}", i, flac.display(), e.replace(',', ";")).expect("write csv row");
                csv.flush().expect("flush csv row");
                failures.push((flac.clone(), e));
            }
        }

        let _ = std::fs::remove_file(&pcm_path);
        let _ = std::fs::remove_file(&mp3_path);
    }

    if let Some((key, group)) = delta_groups.iter().max_by_key(|(_, v)| v.len()) {
        let frac = group.len() as f64 / measurements.len().max(1) as f64;
        if frac >= CANNED_MAJORITY_FRACTION && measurements.len() >= 4 {
            eprintln!("FATAL: {}/{} measured files ({:.0}%) share the EXACT same (Δrms,Δpeak)={:?} pair — canned-output check. Aborting.", group.len(), measurements.len(), frac * 100.0, key);
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
        println!("  outlier threshold (3x |Δ| median) = {outlier_threshold:.3} dB");
        for m in &measurements {
            let v = extract(m).abs();
            if v > outlier_threshold && abs_median > 0.0 {
                println!("  OUTLIER: {} |Δ|={:.3} (> {:.3})", m.path.display(), v, outlier_threshold);
            }
        }
        (abs_median, abs_p95, pos, neg)
    };

    let (rms_abs_median, _rms_abs_p95, _rp, _rn) = report("RMS Δ (ffmpeg - cert)", |m| m.delta_rms);
    let (_peak_abs_median, peak_abs_p95, peak_pos, peak_neg) = report("PEAK Δ (ffmpeg - cert)", |m| m.delta_peak);

    {
        let mut durs: Vec<f64> = measurements.iter().map(|m| m.input_duration_secs).collect();
        durs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("\n--- INPUT DURATION (s), n={} ---", durs.len());
        println!("  min={:.2}  median={:.2}  max={:.2}", durs[0], percentile(&durs, 50.0), durs[durs.len() - 1]);
    }

    println!("\n=== PREDICTION CHECK ===");
    println!("RMS |Δ| median < 0.3 dB: predicted, measured {:.3} dB -> {}", rms_abs_median, if rms_abs_median < 0.3 { "HOLDS" } else { "FALSIFIED" });
    println!("PEAK Δ both signs present: predicted, measured pos={peak_pos} neg={peak_neg} -> {}", if peak_pos > 0 && peak_neg > 0 { "HOLDS" } else { "FALSIFIED" });
    println!("PEAK |Δ| p95 < 1.0 dB: predicted, measured {:.3} dB -> {}", peak_abs_p95, if peak_abs_p95 < 1.0 { "HOLDS" } else { "FALSIFIED" });

    println!("\nelapsed: {:.1}s", started.elapsed().as_secs_f64());
}
