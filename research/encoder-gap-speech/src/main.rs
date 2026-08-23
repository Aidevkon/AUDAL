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

use lineos_types::audio::ManagedPcm;
use m0d::blob_store::{BlobVariant, StoredBlobCore, StoredBlobV2, StoredLoudness, StoredProvenance, StoredQuality, StoredSpatial};
use m0d::dsp::signal_health::DeadAirSummary;
use m0d::handlers::export::export_mp3_acx;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Per-file results are APPENDED here as the run progresses (not
/// batched to the end) — if the session dies mid-run, the measurements
/// taken so far are not lost. Requested 2026-08-23 after the cost
/// estimate step.
const RESULTS_CSV: &str = "/tmp/encoder-gap-speech-results.csv";

/// Fixed seed, declared per F-077 ("γράψε το seed στην αναφορά").
/// LCG constants (Numerical Recipes) — same algorithm already used for
/// deterministic sampling in sp314-dsp/src/analysis/acx_check.rs tests
/// (`noise()` helper), reused here instead of pulling in a `rand` crate
/// for a one-shot reproducible shuffle.
const SEED: u64 = 20260823;
const SAMPLE_TARGET: usize = 60;
const MIN_DURATION_SECS: f64 = 5.0;

struct Lcg(u64);
impl Lcg {
    fn next_u64(&mut self) -> u64 {
        // Numerical Recipes LCG, 64-bit.
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

fn find_all_flacs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|s| s.to_str()) == Some("flac") {
                out.push(p);
            }
        }
    }
    // Deterministic base order — std::fs::read_dir order is NOT
    // guaranteed stable across filesystems/runs.
    out.sort();
    out
}

fn fisher_yates_shuffle<T>(items: &mut [T], rng: &mut Lcg) {
    for i in (1..items.len()).rev() {
        let j = rng.below(i + 1);
        items.swap(i, j);
    }
}

fn ffprobe_duration_secs(path: &Path) -> Option<f64> {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok()
}

/// flac (16 kHz mono, LibriSpeech) -> raw interleaved f32 LE PCM at
/// 48 kHz stereo — the format export_mp3_acx's caller (blob_store)
/// expects (pcm_bytes_to_f32, channels==2 hard requirement,
/// export.rs:529-532). ffmpeg's own resampler does this conversion;
/// export_mp3_acx's OWN 48k->44.1k rubato resample + RMS/peak
/// correction + LAME encode is the thing being measured, untouched.
fn flac_to_48k_stereo_pcm(flac: &Path, pcm_out: &Path) -> Result<(), String> {
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(flac)
        .args(["-ar", "48000", "-ac", "2", "-f", "f32le"])
        .arg(pcm_out)
        .status()
        .map_err(|e| format!("spawn ffmpeg failed: {e}"))?;
    if !status.success() {
        return Err(format!("ffmpeg exited with {status}"));
    }
    Ok(())
}

fn make_blob(id: &str, pcm_path: PathBuf, num_frames: usize) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: id.into(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "2026-08-23T00:00:00Z".into(),
            input_path_hash: "encoder-gap-speech".into(),
            input_pcm_sha256: None,
            seed: SEED,
            pipeline_version: "0.4.0".into(),
            schema_version: 1,
            preset_id: "acx".into(),
            pcm_blake3: None,
            cert_signature: None,
            audio_path: Arc::new(ManagedPcm::new(pcm_path)),
            sample_rate: 48000,
            channels: 2,
            num_frames,
        },
        variant: BlobVariant::Certified {
            loudness: StoredLoudness {
                integrated_lufs: -20.0,
                ..Default::default()
            },
            quality: StoredQuality::default(),
            provenance: StoredProvenance::default(),
            spatial: StoredSpatial::default(),
            stem_fingerprints: None,
            processing_timeline: vec![],
            dead_air: DeadAirSummary::default(),
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            qr_base64: None,
        },
    }
}

/// Same parser as external_acx_ffmpeg_agreement.rs — reused verbatim
/// for methodological consistency with the existing gate.
fn parse_astats_overall(stderr: &str) -> Option<(f64, f64)> {
    let mut peak = None;
    let mut rms = None;
    for line in stderr.lines() {
        if let Some(idx) = line.find("Peak level dB:") {
            peak = line[idx + "Peak level dB:".len()..].trim().parse::<f64>().ok();
        } else if let Some(idx) = line.find("RMS level dB:") {
            rms = line[idx + "RMS level dB:".len()..].trim().parse::<f64>().ok();
        }
    }
    Some((peak?, rms?))
}

fn ffmpeg_astats(mp3: &Path) -> Result<(f64, f64), String> {
    let output = std::process::Command::new("ffmpeg")
        .args(["-i"])
        .arg(mp3)
        .args([
            "-af",
            "astats=measure_overall=Peak_level+RMS_level:measure_perchannel=none",
            "-f", "null", "-",
        ])
        .output()
        .map_err(|e| format!("spawn ffmpeg astats failed: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_astats_overall(&stderr).ok_or_else(|| format!("astats parse failed:\n{stderr}"))
}

/// Percentile via linear interpolation between order statistics
/// (numpy default "linear" method) on an ALREADY-SORTED ascending slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let rank = p / 100.0 * (n as f64 - 1.0);
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = rank - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

struct Measurement {
    path: PathBuf,
    input_duration_secs: f64,
    mp3_bytes: u64,
    cert_rms: f64,
    cert_peak: f64,
    ffmpeg_rms: f64,
    ffmpeg_peak: f64,
    delta_rms: f64,  // ffmpeg - cert, signed (same convention as external_acx_ffmpeg_agreement.rs)
    delta_peak: f64, // ffmpeg - cert, signed
}

fn main() {
    let started = std::time::Instant::now();

    // Optional override of SAMPLE_TARGET via argv[1], for cost-estimation
    // runs (e.g. `encoder-gap-speech 1` to time a single file before
    // committing to the full 60). Sampling order is unaffected — same
    // seed, same shuffle; only how many of the shuffled+filtered list
    // get processed changes.
    let sample_target: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(SAMPLE_TARGET);

    let librispeech_root = PathBuf::from(
        "/home/aidevcon/Downloads/DATASET/librispeech/LibriSpeech/dev-clean",
    );
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
            None => {
                eprintln!("WARN: ffprobe failed on {} — treating as skipped, not counted as <5s", flac.display());
            }
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
        eprintln!(
            "WARN: only {} files selected, target was {sample_target} (corpus scan exhausted or too many short files)",
            selected.len()
        );
    }

    let mut measurements: Vec<Measurement> = Vec::new();
    let mut failures: Vec<(PathBuf, String)> = Vec::new();

    // Append-as-we-go results file — survives a killed session.
    let mut csv = std::fs::File::create(RESULTS_CSV).expect("create results csv");
    writeln!(csv, "idx,path,status,input_duration_secs,mp3_bytes,cert_rms,cert_peak,ffmpeg_rms,ffmpeg_peak,delta_rms,delta_peak,error")
        .expect("write csv header");
    csv.flush().expect("flush csv header");
    println!("results file (append-as-we-go): {RESULTS_CSV}");

    // Sanity check (added 2026-08-23, mid-run, per reviewer request):
    // if a LARGE share of files land on the exact same (Δrms,Δpeak) pair
    // (rounded to 3 decimals — the precision we print/analyze at), that
    // means something is returning canned numbers instead of measuring
    // per-file content. Different material MUST give different Δ.
    // Grouped by exact 3-decimal pair; a small amount of clustering is
    // EXPECTED (this encoder+correction chain converges most quiet
    // narration toward the same RMS-window target, so similar inputs
    // can legitimately land within 0.001 dB of each other) — the bar
    // here is a MAJORITY of the sample sharing one exact pair, which is
    // not explainable by convergence and would only happen if the
    // measurement were not really running per file.
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
            let num_frames = (bytes / 4 / 2) as usize; // f32 (4B) * stereo (2ch)
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

            let (ffmpeg_peak, ffmpeg_rms) = ffmpeg_astats(&mp3_path)?;

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
                    println!(
                        "  NOTE: Δ pair ({:+.3},{:+.3}) shared with {} other file(s) so far (indices {:?})",
                        m.delta_rms, m.delta_peak, group.len() - 1, &group[..group.len() - 1]
                    );
                }

                writeln!(
                    csv, "{},{},ok,{:.3},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},",
                    i, m.path.display(), m.input_duration_secs, m.mp3_bytes,
                    m.cert_rms, m.cert_peak, m.ffmpeg_rms, m.ffmpeg_peak, m.delta_rms, m.delta_peak
                ).expect("write csv row");
                csv.flush().expect("flush csv row");
                measurements.push(m);
            }
            Err(e) => {
                eprintln!("[{}/{}] FAILED {}: {e}", i + 1, total, flac.display());
                writeln!(csv, "{},{},failed,,,,,,,,,{}", i, flac.display(), e.replace(',', ";"))
                    .expect("write csv row");
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
            eprintln!(
                "FATAL: {}/{} measured files ({:.0}%) share the EXACT same (Δrms,Δpeak)={:?} pair \
                 (3-decimal precision) — this looks like canned output, not per-file measurement. \
                 Aborting before computing distribution stats. Indices: {:?}",
                group.len(), measurements.len(), frac * 100.0, key, group
            );
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
        println!(
            "  min={:.2}  median={:.2}  max={:.2}",
            durs[0], percentile(&durs, 50.0), durs[durs.len() - 1]
        );
        println!(
            "  DECLARED LIMIT: this sample is short-utterance speech (LibriSpeech dev-clean, \
             median ~{:.1}s). An RMS/peak encoder gap is a valid finding regardless of \
             duration (energy and sample peak do not depend on how long the file is) — but \
             this sample says NOTHING about noise-floor behavior on long-form material with \
             many pauses (a 3-hour audiobook chapter). The ACX noise floor is the quietest \
             sliding 500ms window; a ~7s utterance has little to no room tone to find that \
             window in. This is a limit of the MATERIAL, not of the measurement.",
            percentile(&durs, 50.0)
        );
    }

    println!("\n=== PREDICTION CHECK ===");
    println!(
        "RMS |Δ| median < 0.3 dB: predicted, measured {:.3} dB -> {}",
        rms_abs_median,
        if rms_abs_median < 0.3 { "HOLDS" } else { "FALSIFIED" }
    );
    println!(
        "PEAK Δ both signs present: predicted, measured pos={peak_pos} neg={peak_neg} -> {}",
        if peak_pos > 0 && peak_neg > 0 { "HOLDS" } else { "FALSIFIED" }
    );
    println!(
        "PEAK |Δ| p95 < 1.0 dB: predicted, measured {:.3} dB -> {}",
        peak_abs_p95,
        if peak_abs_p95 < 1.0 { "HOLDS" } else { "FALSIFIED" }
    );

    println!("\nelapsed: {:.1}s", started.elapsed().as_secs_f64());
}
