use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

#[derive(Serialize)]
struct FeaturesOutput {
    format: &'static str,
    format_version: u32,
    engine_commit: &'static str,
    source_file: String,
    sha256: String,
    sample_rate: u32,
    duration_s: f32,
    features: ExtractedFeatures,
}

#[derive(Serialize)]
struct ExtractedFeatures {
    integrated_lufs: f32,
    rms_db: f32,
    crest_db: f32,
    lra_lu: f32,
    dynamic_range_db: f32,
    acx_noise_floor_proxy_db: f32,
    noise_floor_dbfs: Option<f32>,
    spectral_profile_db: [f32; 8],
    transient_density: f32,
    zcr_mean: f32,
    zcr_std: f32,
    bpm_estimate: f32,
    bpm_confidence: f32,
    global_phase_correlation: f32,
    cv_ioi_sequence: Vec<f32>,
    cepstral_flux_sequence: Vec<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_ratio: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_posterior_mean: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_posterior_std: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_longest_run_s: Option<f32>,
    mfcc_1_mean: f32,
    mfcc_1_std: f32,
    mfcc_2_mean: f32,
    mfcc_2_std: f32,
    mfcc_3_mean: f32,
    mfcc_3_std: f32,
    mfcc_4_mean: f32,
    mfcc_4_std: f32,
    mfcc_5_mean: f32,
    mfcc_5_std: f32,
    mfcc_6_mean: f32,
    mfcc_6_std: f32,
    mfcc_7_mean: f32,
    mfcc_7_std: f32,
    mfcc_8_mean: f32,
    mfcc_8_std: f32,
    mfcc_9_mean: f32,
    mfcc_9_std: f32,
    mfcc_10_mean: f32,
    mfcc_10_std: f32,
    mfcc_11_mean: f32,
    mfcc_11_std: f32,
    mfcc_12_mean: f32,
    mfcc_12_std: f32,
    mfcc_13_mean: f32,
    mfcc_13_std: f32,
}

fn process_file(input: &Path, out_dir: &Path, enable_vad: bool) -> Result<(), String> {
    eprintln!("Processing {}", input.display());

    // 1. Create a temporary dump file for the raw PCM.
    let temp_file = NamedTempFile::new().map_err(|e| format!("Failed to create temp file: {}", e))?;
    let temp_path = temp_file.path().to_str().unwrap().to_string();

    // 2. Decode to dump and get metrics.
    // This uses StandardizedDecoder which resamples to 48kHz, converts to stereo, sanitizes.
    let (_, decoder) = m0d::dsp::input_lufs::pass0_decode_to_dump(input, &temp_path)?;
    let sha256 = decoder.input_hashes().1;
    let expected_frames = decoder.expected_output_frames().unwrap_or(0);
    let duration_s = expected_frames as f32 / 48000.0;

    // 3. Run Trunk Pass on the raw dump.
    let trunk_report = sp314_orchestrator::trunk_pass::run_trunk_pass(temp_file.path(), enable_vad)?;

    // 4. Map into explicit schema.
    let m = &trunk_report.metrics;
    let output = FeaturesOutput {
        format: "creator-os-features",
        format_version: 1,
        engine_commit: env!("GIT_HASH"),
        source_file: input.file_name().unwrap_or_default().to_string_lossy().to_string(),
        sha256,
        sample_rate: 48000,
        duration_s,
        features: ExtractedFeatures {
            integrated_lufs: m.integrated_lufs.unwrap_or(-144.0),
            rms_db: m.rms_db,
            crest_db: m.crest_db,
            lra_lu: m.lra,
            dynamic_range_db: m.dynamic_range_db,
            acx_noise_floor_proxy_db: m.acx_noise_floor_proxy_db,
            noise_floor_dbfs: m.noise_floor_dbfs,
            spectral_profile_db: m.spectral_profile_db,
            transient_density: m.transient_density,
            zcr_mean: m.zcr_mean,
            zcr_std: m.zcr_std,
            bpm_estimate: m.bpm_estimate,
            bpm_confidence: m.bpm_confidence,
            global_phase_correlation: m.global_phase_correlation,
            cv_ioi_sequence: m.cv_ioi_sequence.clone(),
            cepstral_flux_sequence: m.cepstral_flux_sequence.clone(),
            voice_ratio: m.voice_ratio,
            voice_posterior_mean: m.voice_posterior_mean,
            voice_posterior_std: m.voice_posterior_std,
            voice_longest_run_s: m.voice_longest_run_s,
            mfcc_1_mean: m.mfcc_means[0],
            mfcc_1_std: m.mfcc_stds[0],
            mfcc_2_mean: m.mfcc_means[1],
            mfcc_2_std: m.mfcc_stds[1],
            mfcc_3_mean: m.mfcc_means[2],
            mfcc_3_std: m.mfcc_stds[2],
            mfcc_4_mean: m.mfcc_means[3],
            mfcc_4_std: m.mfcc_stds[3],
            mfcc_5_mean: m.mfcc_means[4],
            mfcc_5_std: m.mfcc_stds[4],
            mfcc_6_mean: m.mfcc_means[5],
            mfcc_6_std: m.mfcc_stds[5],
            mfcc_7_mean: m.mfcc_means[6],
            mfcc_7_std: m.mfcc_stds[6],
            mfcc_8_mean: m.mfcc_means[7],
            mfcc_8_std: m.mfcc_stds[7],
            mfcc_9_mean: m.mfcc_means[8],
            mfcc_9_std: m.mfcc_stds[8],
            mfcc_10_mean: m.mfcc_means[9],
            mfcc_10_std: m.mfcc_stds[9],
            mfcc_11_mean: m.mfcc_means[10],
            mfcc_11_std: m.mfcc_stds[10],
            mfcc_12_mean: m.mfcc_means[11],
            mfcc_12_std: m.mfcc_stds[11],
            mfcc_13_mean: m.mfcc_means[12],
            mfcc_13_std: m.mfcc_stds[12],
        },
    };

    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let out_file = out_dir.join(format!("{}.features.json", stem));

    let json = serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
    std::fs::write(&out_file, json).map_err(|e| format!("Write error to {}: {}", out_file.display(), e))?;

    eprintln!(" -> Wrote {}", out_file.display());
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut input_path = None;
    let mut out_dir = None;
    let mut enable_vad = false;

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--out" && i + 1 < args.len() {
            out_dir = Some(PathBuf::from(&args[i + 1]));
            i += 2;
        } else if args[i] == "--vad" {
            enable_vad = true;
            i += 1;
        } else {
            input_path = Some(PathBuf::from(&args[i]));
            i += 1;
        }
    }

    let input = match input_path {
        Some(p) => p,
        None => {
            eprintln!("Usage: features <input_file_or_dir> [--out <dir>] [--vad]");
            std::process::exit(1);
        }
    };

    let out_dir = out_dir.unwrap_or_else(|| PathBuf::from("."));
    if !out_dir.exists() {
        std::fs::create_dir_all(&out_dir).expect("Failed to create output directory");
    }

    let mut files_to_process = Vec::new();
    if input.is_dir() {
        for entry in std::fs::read_dir(&input).expect("Failed to read input dir") {
            if let Ok(entry) = entry {
                let p = entry.path();
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "wav" || ext_lower == "mp3" || ext_lower == "flac" {
                        files_to_process.push(p);
                    }
                }
            }
        }
    } else {
        files_to_process.push(input);
    }

    let mut has_errors = false;
    for file in files_to_process {
        if let Err(e) = process_file(&file, &out_dir, enable_vad) {
            eprintln!("Error processing {}: {}", file.display(), e);
            has_errors = true;
        }
    }

    if has_errors {
        std::process::exit(1);
    }
}
