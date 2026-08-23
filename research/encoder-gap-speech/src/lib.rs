//! Shared measurement infrastructure for the F-077 encoder-gap
//! experiments (main.rs = round 1, lufs_round2.rs = round 2,
//! gap_mechanism_*.rs = round 3). Extracted 2026-08-23 so a fix to one
//! instrument (e.g. the rebuild_pre_encode_mono chain, or the ebur128
//! parser) cannot silently diverge between binaries — round 2 already
//! hit two bugs from copy-pasted logic; this crate stays single-source
//! from here on. Nothing here touches product code (m0d/sp314-dsp) —
//! it only CALLS their public API, same as every binary already did.

use lineos_types::audio::ManagedPcm;
use m0d::blob_store::{
    BlobVariant, StoredBlobCore, StoredBlobV2, StoredLoudness, StoredProvenance, StoredQuality,
    StoredSpatial,
};
use m0d::dsp::signal_health::DeadAirSummary;
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use sp314_dsp::analysis::acx_check::{AcxCheckAnalyzer, ACX_MAX_RMS_DB, ACX_MIN_RMS_DB};
use sp314_dsp::metering::true_peak_meter::TruePeakMeter;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Fixed seed, declared per F-077 ("γράψε το seed στην αναφορά").
pub const SEED: u64 = 20260823;
pub const SAMPLE_TARGET: usize = 60;
pub const MIN_DURATION_SECS: f64 = 5.0;

/// export.rs:642 — ιδιωτική σταθερά, ΟΧΙ pub. Αντιγραμμένη με το χέρι,
/// δηλωμένο εδώ (γύρος 2).
pub const RMS_MARGIN_DB: f32 = 0.5;

pub struct Lcg(pub u64);
impl Lcg {
    pub fn next_u64(&mut self) -> u64 {
        // Numerical Recipes LCG, 64-bit.
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

pub fn find_all_flacs(root: &Path) -> Vec<PathBuf> {
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
    out.sort();
    out
}

pub fn fisher_yates_shuffle<T>(items: &mut [T], rng: &mut Lcg) {
    for i in (1..items.len()).rev() {
        let j = rng.below(i + 1);
        items.swap(i, j);
    }
}

pub fn ffprobe_duration_secs(path: &Path) -> Option<f64> {
    let out = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
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
/// export.rs:529-532).
pub fn flac_to_48k_stereo_pcm(flac: &Path, pcm_out: &Path) -> Result<(), String> {
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

pub fn make_blob(id: &str, pcm_path: PathBuf, num_frames: usize) -> StoredBlobV2 {
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
            loudness: StoredLoudness { integrated_lufs: -20.0, ..Default::default() },
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

/// Ξαναχτισμένο pre-LAME mono buffer, 44.1kHz — ΤΑΥΤΟΣΗΜΑ βήματα με
/// export.rs (βλ. .reports/2026-08-23-encoder-gap-lufs.md §"ΤΙ ΕΓΙΝΕ"
/// για τον πίνακα γραμμών). Ελεγμένο 60/60 έναντι του πραγματικού
/// export_mp3_acx report με ανοχή 0.01 dB (γύρος 2).
pub fn rebuild_pre_encode_mono(planar_48k: &[Vec<f32>; 2]) -> Vec<f32> {
    let original_sr = 48000;
    let target_sr = 44100;
    let ratio = target_sr as f64 / original_sr as f64;
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, 4096, 2)
        .expect("resampler init (same params as export.rs — must succeed identically)");

    let total_frames = planar_48k[0].len();
    let mut pos = 0usize;
    let mut resampled_planar: Vec<Vec<f32>> = vec![Vec::new(); 2];
    while pos < total_frames {
        let end = (pos + 4096).min(total_frames);
        let chunk_len = end - pos;
        let wave_in: Vec<Vec<f32>> = if chunk_len == 4096 {
            planar_48k.iter().map(|ch| ch[pos..end].to_vec()).collect()
        } else {
            planar_48k
                .iter()
                .map(|ch| {
                    let mut v = ch[pos..end].to_vec();
                    v.resize(4096, 0.0);
                    v
                })
                .collect()
        };
        let mut wave_out = resampler.process(&wave_in, None).expect("resample chunk");
        let valid_out_frames = (chunk_len as f64 * ratio).round() as usize;
        for c in 0..2 {
            wave_out[c].truncate(valid_out_frames);
            resampled_planar[c].extend_from_slice(&wave_out[c]);
        }
        pos += chunk_len;
    }

    let mut mono: Vec<f32> = resampled_planar[0]
        .iter()
        .zip(resampled_planar[1].iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();

    let mut acx_pre = AcxCheckAnalyzer::new(target_sr as u32);
    for chunk in mono.chunks(4096) {
        acx_pre.feed_chunk(chunk);
    }
    let rms_before = acx_pre.finish().rms_db;

    let rms_correction_db = if rms_before < ACX_MIN_RMS_DB {
        ACX_MIN_RMS_DB + RMS_MARGIN_DB - rms_before
    } else if rms_before > ACX_MAX_RMS_DB {
        ACX_MAX_RMS_DB - RMS_MARGIN_DB - rms_before
    } else {
        0.0
    };
    if rms_correction_db != 0.0 {
        let gain = libm::powf(10.0, rms_correction_db / 20.0);
        for s in mono.iter_mut() {
            *s *= gain;
        }
    }

    let mut tp_meter = TruePeakMeter::new();
    for chunk in mono.chunks(4096) {
        tp_meter.process_chunk(chunk, chunk);
    }
    let tp_db = tp_meter.finish();
    if tp_db > -3.0 {
        let diff_db = -3.05 - tp_db;
        let gain = libm::powf(10.0, diff_db / 20.0);
        for s in mono.iter_mut() {
            *s *= gain;
        }
    }

    mono
}

pub fn write_f32le(samples: &[f32], path: &Path) -> Result<(), String> {
    let mut f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    for &s in samples {
        f.write_all(&s.to_le_bytes()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// ΔΙΟΡΘΩΜΕΝΟΣ parser (γύρος 2, §"ΔΥΟ ΔΙΟΡΘΩΣΕΙΣ"): το ανθρωπο-
/// αναγνώσιμο Summary block είναι κλειδωμένο σε 1 δεκαδικό από το ίδιο
/// το ffmpeg. `ebur128=peak=true:metadata=1,ametadata=print:file=-`
/// εκθέτει τις ΙΔΙΕΣ τιμές ως frame metadata σε STDOUT με 3 δεκαδικά
/// (`lavfi.r128.I=` σε LUFS, `lavfi.r128.true_peak=` σε ΓΡΑΜΜΙΚΟ
/// πλάτος — χρειάζεται 20*log10). Παίρνει την ΤΕΛΕΥΤΑΙΑ τιμή κάθε
/// κλειδιού (το τελικό αθροιστικό block).
pub fn parse_ebur128_metadata(stdout: &str) -> Option<(f64, f64)> {
    let mut integrated = None;
    let mut true_peak_linear = None;
    for line in stdout.lines() {
        if let Some(v) = line.strip_prefix("lavfi.r128.I=") {
            if let Ok(f) = v.trim().parse::<f64>() {
                integrated = Some(f);
            }
        } else if let Some(v) = line.strip_prefix("lavfi.r128.true_peak=") {
            if let Ok(f) = v.trim().parse::<f64>() {
                true_peak_linear = Some(f);
            }
        }
    }
    let i = integrated?;
    let tp_linear = true_peak_linear?;
    let tp_db = if tp_linear < 1e-10 { -144.0 } else { 20.0 * tp_linear.log10() };
    Some((i, tp_db))
}

/// Returns (integrated_lufs, true_peak_dbtp). ΠΡΟΣΟΧΗ: ΠΟΤΕ "-v error"/
/// "-loglevel error" εδώ — σβήνει σιωπηλά το metadata/Summary output
/// (γύρος 2 bug #1). "-hide_banner" είναι ασφαλές.
pub fn ffmpeg_ebur128_on_file(ffmpeg_input_args: &[&str], input: &Path) -> Result<(f64, f64), String> {
    let output = std::process::Command::new("ffmpeg")
        .args(ffmpeg_input_args)
        .arg("-i")
        .arg(input)
        .args(["-af", "ebur128=peak=true:metadata=1,ametadata=print:file=-", "-f", "null", "-"])
        .output()
        .map_err(|e| format!("spawn ffmpeg ebur128 failed: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ebur128_metadata(&stdout).ok_or_else(|| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!("ebur128 metadata parse failed. stdout:\n{stdout}\nstderr:\n{stderr}")
    })
}

/// Same parser as external_acx_ffmpeg_agreement.rs — reused verbatim
/// for methodological consistency with the existing gate. Returns
/// (peak_db, rms_db) — unweighted sample-domain, NOT true peak/LUFS.
pub fn parse_astats_overall(stderr: &str) -> Option<(f64, f64)> {
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

/// Returns (peak_db, rms_db) via ffmpeg astats.
pub fn ffmpeg_astats(ffmpeg_input_args: &[&str], input: &Path) -> Result<(f64, f64), String> {
    let output = std::process::Command::new("ffmpeg")
        .args(ffmpeg_input_args)
        .arg("-i")
        .arg(input)
        .args(["-af", "astats=measure_overall=Peak_level+RMS_level:measure_perchannel=none", "-f", "null", "-"])
        .output()
        .map_err(|e| format!("spawn ffmpeg astats failed: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_astats_overall(&stderr).ok_or_else(|| format!("astats parse failed:\n{stderr}"))
}

/// Percentile via linear interpolation between order statistics
/// (numpy default "linear" method) on an ALREADY-SORTED ascending slice.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
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

/// AcxCheckAnalyzer, καλυμμένο για μονοφωνικό σήμα σε ΟΠΟΙΟΔΗΠΟΤΕ
/// sample rate — χρησιμοποιείται και για cert-side RMS/peak (44100)
/// και για oracle-side μετρήσεις.
pub fn measure_rms_peak(mono: &[f32], sample_rate: u32) -> (f64, f64) {
    let mut a = AcxCheckAnalyzer::new(sample_rate);
    for chunk in mono.chunks(4096) {
        a.feed_chunk(chunk);
    }
    let r = a.finish();
    (r.rms_db as f64, r.sample_peak_db as f64)
}
