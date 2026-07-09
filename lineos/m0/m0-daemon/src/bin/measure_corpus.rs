use lineos_corpus::genre_centroid::{
    compute_bucket_centroid, compute_global_stats, format_f32_const, gate, RejectReason,
    TrackGateInputs, GATE_V1_ACOUSTIC, GATE_V1_IDM,
};
use lineos_corpus::mfcc::{MfccAnalyzer, FFT_SIZE, N_MFCC};
use m0d::handlers::decode::decode_audio;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sp314_dsp::analysis::dynamics::crest_factor_db;
use sp314_dsp::analysis::{loudness_range_lu, spectral_profile_levels, spectral_slope};
use sp314_dsp::metering::measure_integrated_lufs;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// F-023: The old measure_genre_centroids tool had a redundant second loop. Fixed here to one loop.
const SILENCE_THRESHOLD_DB: f32 = -60.0; // Copied from old measure_genre_centroids tool
const MIN_TRACKS_PER_BUCKET: usize = 15;

const BAND_EDGES: [f32; 9] = [
    20.0, 80.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 20000.0,
];
const BAND_LABELS: [&str; 8] = [
    "sub", "bass", "low_mid", "mid_low", "mid_high", "high_mid", "treble", "air",
];

struct TrackResult {
    path: PathBuf,
    sha256: String,
    lufs: f32,
    crest_db: f32,
    slope: f32,
    lra_lu: f32,
    levels_db: [f32; 8],
    verdict: Result<(), Vec<RejectReason>>,
    mfcc_frames: Vec<[f32; N_MFCC]>,
    mfcc_frames_count: usize,
}

struct BucketResult {
    name: String,
    tracks: Vec<TrackResult>,
    decode_failures: Vec<(PathBuf, String)>,
}

#[derive(Serialize)]
struct ManifestTrack {
    path: String,
    sha256: String,
    lufs: f32,
    crest_db: f32,
    slope: f32,
    lra_lu: f32,
}

#[derive(Serialize)]
struct ManifestReject {
    path: String,
    sha256: String,
    reasons: Vec<String>,
}

#[derive(Serialize)]
struct ManifestDecodeFailure {
    path: String,
    error: String,
}

#[derive(Serialize)]
struct ManifestBucket {
    accepted: Vec<ManifestTrack>,
    rejected: Vec<ManifestReject>,
    decode_failures: Vec<ManifestDecodeFailure>,
}

#[derive(Serialize)]
struct ManifestRubato {
    sinc_len: u32,
    f_cutoff: f32,
    interpolation: String,
    oversampling: u32,
    window: String,
}

#[derive(Serialize)]
struct CorpusManifest {
    corpus_version: String,
    gate_version: String,
    gate_criteria: serde_json::Value,
    tool_git_commit: String,
    decode_path: String,
    rubato_provenance: ManifestRubato,
    buckets: std::collections::BTreeMap<String, ManifestBucket>,
}

#[derive(Serialize)]
struct ProfileCorpus {
    source: String,
    version: String,
    manifest_hash: String,
}

#[derive(Serialize)]
struct ProfileHardConstraints {
    target_lufs: f32,
    true_peak_ceiling_dbtp: f32,
    gate_absolute_lufs: f32,
    gate_relative_lu: f32,
    lra_target_lu: f32,
}

#[derive(Serialize)]
struct ProfileBand {
    index: usize,
    label: String,
    range_hz: [f32; 2],
    target_db_relative: f32,
}

#[derive(Serialize)]
struct ProfileSpectralTarget {
    normalization_band_count: usize,
    bands: Vec<ProfileBand>,
}

#[derive(Serialize)]
struct ProfileSbr {
    lower_band_index: usize,
    upper_band_index: usize,
    lower_band_hz: [f32; 2],
    upper_band_hz: [f32; 2],
    _comment: String,
}

#[derive(Serialize)]
struct ProfileRoot {
    id: String,
    _schema_version: u32,
    g_max_db: f32,
    dead_zone_db: [f32; 8],
    corpus: ProfileCorpus,
    hard_constraints: ProfileHardConstraints,
    spectral_target: ProfileSpectralTarget,
    sbr: ProfileSbr,
}

fn rms_db(chunk: &[f32]) -> f32 {
    if chunk.is_empty() {
        return -f32::INFINITY;
    }
    let mut sum_sq = 0.0;
    for &s in chunk {
        sum_sq += s * s;
    }
    let rms = libm::sqrtf(sum_sq / chunk.len() as f32);
    if rms < 1e-10 {
        -f32::INFINITY
    } else {
        20.0 * libm::log10f(rms)
    }
}

fn get_git_head() -> String {
    if let Ok(output) = Command::new("git").args(["rev-parse", "HEAD"]).output() {
        if output.status.success() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                return s.trim().to_string();
            }
        }
    }
    eprintln!("WARNING: Failed to get git HEAD commit");
    "unknown".to_string()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 5 || args[3] != "--corpus-version" {
        eprintln!(
            "Usage: {} <corpus_dir> <output_dir> --corpus-version <tag>",
            args[0]
        );
        std::process::exit(1);
    }
    let corpus_dir = Path::new(&args[1]);
    let output_dir = Path::new(&args[2]);
    let corpus_version = &args[4];

    if !corpus_dir.exists() || !corpus_dir.is_dir() {
        eprintln!("Error: corpus_dir does not exist or is not a directory");
        std::process::exit(1);
    }

    if !output_dir.exists() {
        fs::create_dir_all(output_dir).expect("Failed to create output_dir");
    }

    let mut buckets_info = Vec::new();
    let entries = fs::read_dir(corpus_dir).unwrap();
    let mut has_buckets = false;
    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            has_buckets = true;
            let name = entry.file_name().to_string_lossy().to_string();
            let gate_criteria = match name.as_str() {
                "idm" => &GATE_V1_IDM,
                "acoustic" => &GATE_V1_ACOUSTIC,
                _ => {
                    eprintln!(
                        "Error: Unknown bucket '{}'. F-024 lesson: no silent catch-all.",
                        name
                    );
                    std::process::exit(1);
                }
            };
            buckets_info.push((name, path, gate_criteria));
        }
    }

    if !has_buckets {
        eprintln!("Error: Empty corpus_dir (no bucket subdirectories)");
        std::process::exit(1);
    }

    // Sort buckets by name for determinism
    buckets_info.sort_by(|a, b| a.0.cmp(&b.0));

    let mut all_results = Vec::new();
    let git_head = get_git_head();

    for (bucket_name, bucket_path, gate_criteria) in &buckets_info {
        println!("Processing bucket: {}", bucket_name);
        let mut files: Vec<_> = fs::read_dir(bucket_path)
            .unwrap()
            .map(|e| e.unwrap())
            .collect();
        // Sort files by name for determinism
        files.sort_by_key(|a| a.file_name());

        let mut track_results = Vec::new();
        let mut decode_failures = Vec::new();

        for file in files {
            let path = file.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if ext == "wav" || ext == "mp3" || ext == "flac" {
                        let bytes = fs::read(&path).unwrap();
                        let mut hasher = Sha256::new();
                        hasher.update(&bytes);
                        let hash = hex::encode(hasher.finalize());

                        // a. decode_audio(path)
                        let pcm = match decode_audio(path.to_str().unwrap()) {
                            Ok(p) => p,
                            Err(e) => {
                                let err_str = format!("{:?}", e);
                                eprintln!("Warning: Failed to decode {:?}: {}", path, err_str);
                                decode_failures.push((path.clone(), err_str));
                                continue;
                            }
                        };
                        let interleaved = pcm.samples;

                        // b. de-interleave
                        let mut left = Vec::with_capacity(interleaved.len() / 2);
                        let mut right = Vec::with_capacity(interleaved.len() / 2);
                        for chunk in interleaved.chunks_exact(2) {
                            left.push(chunk[0]);
                            right.push(chunk[1]);
                        }

                        // c. mono
                        let mut mono = Vec::with_capacity(left.len());
                        for i in 0..left.len() {
                            mono.push((left[i] + right[i]) * 0.5);
                        }

                        // d. measurements
                        let lufs = measure_integrated_lufs(&left, &right);
                        let crest_db = crest_factor_db(&mono);
                        let levels_db = spectral_profile_levels(&left, &right, 48000);
                        let slope = spectral_slope(&levels_db);
                        let lra_lu = loudness_range_lu(&left, &right);

                        // e. gate check
                        let inputs = TrackGateInputs {
                            lufs,
                            crest_db,
                            slope,
                        };
                        let verdict = gate(&inputs, gate_criteria);

                        let mut mfcc_frames_count = 0;
                        let mut mfcc_frames = Vec::new();
                        if verdict.is_ok() {
                            // f. extract MFCC frames
                            let mut mfcc_analyzer = MfccAnalyzer::new();
                            for chunk in mono.chunks(FFT_SIZE) {
                                if chunk.len() == FFT_SIZE {
                                    let db = rms_db(chunk);
                                    if db >= SILENCE_THRESHOLD_DB {
                                        let features = mfcc_analyzer.compute(chunk);
                                        mfcc_frames.push(features);
                                        mfcc_frames_count += 1;
                                    }
                                }
                            }
                        }

                        track_results.push(TrackResult {
                            path,
                            sha256: hash,
                            lufs,
                            crest_db,
                            slope,
                            lra_lu,
                            levels_db,
                            verdict,
                            mfcc_frames,
                            mfcc_frames_count,
                        });
                    }
                }
            }
        }
        if track_results.is_empty() {
            eprintln!("Error: Bucket '{}' has zero audio files", bucket_name);
            std::process::exit(1);
        }
        all_results.push(BucketResult {
            name: bucket_name.clone(),
            tracks: track_results,
            decode_failures,
        });
    }

    // --- Aggregation (accepted tracks only) ---

    let mut global_accepted_frames = Vec::new();
    for bucket in &all_results {
        let accepted = bucket.tracks.iter().filter(|t| t.verdict.is_ok()).count();
        if accepted == 0 {
            eprintln!("Error: Bucket '{}' has zero accepted tracks", bucket.name);
            std::process::exit(1);
        }
        for track in &bucket.tracks {
            if track.verdict.is_ok() {
                global_accepted_frames.extend(track.mfcc_frames.clone());
            }
        }
    }

    // d. Global MFCC stats
    let (global_mean, global_std) = compute_global_stats(&global_accepted_frames).unwrap();
    let mut bucket_centroids = Vec::new();

    for bucket in &all_results {
        let mut bucket_frames = Vec::new();
        for track in &bucket.tracks {
            if track.verdict.is_ok() {
                bucket_frames.extend(track.mfcc_frames.clone());
            }
        }
        let (z_mean, _z_var) =
            compute_bucket_centroid(&bucket_frames, &global_mean, &global_std).unwrap();
        bucket_centroids.push((bucket.name.clone(), z_mean));
    }

    // Write genre_centroids_generated.rs
    let mut rs_out = String::new();
    rs_out.push_str(&format!(
        "// Generated by measure_corpus (corpus version: {})\n",
        corpus_version
    ));
    rs_out.push_str("// DO NOT hand-edit.\n\n");
    rs_out.push_str("pub const GLOBAL_MFCC_MEAN: [f32; 13] = [\n");
    for v in &global_mean {
        rs_out.push_str(&format!("    {},\n", format_f32_const(*v)));
    }
    rs_out.push_str("];\n\n");
    rs_out.push_str("pub const GLOBAL_MFCC_STD: [f32; 13] = [\n");
    for v in &global_std {
        rs_out.push_str(&format!("    {},\n", format_f32_const(*v)));
    }
    rs_out.push_str("];\n\n");

    for (name, centroid) in &bucket_centroids {
        let const_name = format!("{}_MFCC_MEAN", name.to_uppercase());
        rs_out.push_str(&format!("pub const {}: [f32; 13] = [\n", const_name));
        for v in centroid {
            rs_out.push_str(&format!("    {},\n", format_f32_const(*v)));
        }
        rs_out.push_str("];\n\n");
    }
    fs::write(output_dir.join("genre_centroids_generated.rs"), rs_out).unwrap();

    // j. corpus-manifest.json
    let mut manifest_buckets = std::collections::BTreeMap::new();
    for bucket in &all_results {
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        for track in &bucket.tracks {
            let rel_path = track
                .path
                .strip_prefix(corpus_dir)
                .unwrap_or(&track.path)
                .to_string_lossy()
                .to_string();
            match &track.verdict {
                Ok(_) => {
                    accepted.push(ManifestTrack {
                        path: rel_path,
                        sha256: track.sha256.clone(),
                        lufs: track.lufs,
                        crest_db: track.crest_db,
                        slope: track.slope,
                        lra_lu: track.lra_lu,
                    });
                }
                Err(reasons) => {
                    rejected.push(ManifestReject {
                        path: rel_path,
                        sha256: track.sha256.clone(),
                        reasons: reasons.iter().map(|r| format!("{:?}", r)).collect(),
                    });
                }
            }
        }
        let dfs = bucket
            .decode_failures
            .iter()
            .map(|(p, e)| ManifestDecodeFailure {
                path: p
                    .strip_prefix(corpus_dir)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string(),
                error: e.clone(),
            })
            .collect();

        manifest_buckets.insert(
            bucket.name.clone(),
            ManifestBucket {
                accepted,
                rejected,
                decode_failures: dfs,
            },
        );
    }

    let manifest = CorpusManifest {
        corpus_version: corpus_version.to_string(),
        gate_version: "gate-v1.1".to_string(), // acoustic retuned to v1.1 2026-07-09; idm still gate-v1 — bump this string wholesale when idm gets its own real-corpus retune.
        gate_criteria: serde_json::json!({
            "idm": { "lufs_min": -14.0, "lufs_max": -6.0, "crest_min_db": 4.0, "slope_min": -1.05, "slope_max": -0.45 },
            "acoustic": { "lufs_min": -24.0, "lufs_max": -10.0, "crest_min_db": 8.0, "slope_min": -1.60, "slope_max": -0.85 }
        }),
        tool_git_commit: git_head,
        decode_path: "decode_audio (batch symphonia+rubato)".to_string(),
        rubato_provenance: ManifestRubato {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: "Linear".to_string(),
            oversampling: 256,
            window: "BlackmanHarris2".to_string(),
        },
        buckets: manifest_buckets,
    };

    let manifest_str = serde_json::to_string_pretty(&manifest).unwrap();
    fs::write(output_dir.join("corpus-manifest.json"), &manifest_str).unwrap();

    let mut hasher = Sha256::new();
    hasher.update(manifest_str.as_bytes());
    let manifest_hash = hex::encode(hasher.finalize());

    // Write profiles
    for bucket in &all_results {
        // e. Per-bucket spectral target
        let mut target_db = [0.0; 8];
        let mut dead_zone_db = [0.0; 8];
        let accepted_tracks: Vec<_> = bucket.tracks.iter().filter(|t| t.verdict.is_ok()).collect();
        let n_accepted = accepted_tracks.len() as f32;

        let mut mean_centered_shapes = Vec::new();

        for track in &accepted_tracks {
            let mean = track.levels_db.iter().sum::<f32>() / 8.0;
            let mut centered = [0.0; 8];
            for k in 0..8 {
                centered[k] = track.levels_db[k] - mean;
            }
            mean_centered_shapes.push(centered);
        }

        for k in 0..8 {
            let mut sum_energy = 0.0;
            for shape in &mean_centered_shapes {
                sum_energy += libm::powf(10.0, shape[k] / 10.0);
            }
            let avg_energy = sum_energy / n_accepted;
            target_db[k] = 10.0 * libm::log10f(avg_energy);
        }

        let target_mean = target_db.iter().sum::<f32>() / 8.0;
        for k in 0..8 {
            target_db[k] -= target_mean;
        }

        // target = where we pull toward (energy-weighted); dead zone = population spread around its arithmetic center; different questions, different centers.
        for k in 0..8 {
            let mut shape_sum = 0.0;
            for shape in &mean_centered_shapes {
                shape_sum += shape[k];
            }
            let shape_mean = shape_sum / n_accepted;

            let mut sum_sq = 0.0;
            for shape in &mean_centered_shapes {
                let diff = shape[k] - shape_mean;
                sum_sq += diff * diff;
            }
            dead_zone_db[k] = libm::sqrtf(sum_sq / n_accepted);
        }

        // g. lra_target_lu
        let mut lra_vals: Vec<_> = accepted_tracks.iter().map(|t| t.lra_lu).collect();
        lra_vals.sort_by(|a, b| a.total_cmp(b));
        let lra_target_lu = if lra_vals.len() % 2 == 0 {
            // arithmetic mean of the two middle values — for determinism
            (lra_vals[lra_vals.len() / 2 - 1] + lra_vals[lra_vals.len() / 2]) / 2.0
        } else {
            lra_vals[lra_vals.len() / 2]
        };

        let mut profile_bands = Vec::new();
        for k in 0..8 {
            profile_bands.push(ProfileBand {
                index: k,
                label: BAND_LABELS[k].to_string(),
                range_hz: [BAND_EDGES[k], BAND_EDGES[k + 1]],
                target_db_relative: target_db[k],
            });
        }

        let profile = ProfileRoot {
            id: format!("music-{}-v1", bucket.name),
            _schema_version: 2,
            g_max_db: 2.5,
            dead_zone_db,
            corpus: ProfileCorpus {
                source: "genre-corpus".to_string(),
                version: corpus_version.to_string(),
                manifest_hash: manifest_hash.clone(),
            },
            hard_constraints: ProfileHardConstraints {
                target_lufs: -14.0,
                true_peak_ceiling_dbtp: -1.0,
                gate_absolute_lufs: -70.0,
                gate_relative_lu: -10.0,
                lra_target_lu,
            },
            spectral_target: ProfileSpectralTarget {
                normalization_band_count: 8,
                bands: profile_bands,
            },
            sbr: ProfileSbr {
                lower_band_index: 3,
                upper_band_index: 4,
                lower_band_hz: [BAND_EDGES[3], BAND_EDGES[4]],
                upper_band_hz: [BAND_EDGES[4], BAND_EDGES[5]],
                _comment: "inherited band pair — not used in the music resolve path today (F-025)"
                    .to_string(),
            },
        };

        let profile_str = serde_json::to_string_pretty(&profile).unwrap();
        fs::write(
            output_dir.join(format!("music-{}-v1.json", bucket.name)),
            &profile_str,
        )
        .unwrap();
    }

    println!("\n=== End of Run Report ===");
    for bucket in &all_results {
        let accepted = bucket.tracks.iter().filter(|t| t.verdict.is_ok()).count();
        let rejected = bucket.tracks.len() - accepted;
        println!(
            "\nBucket [{}]: {} accepted, {} rejected, {} decode failures",
            bucket.name,
            accepted,
            rejected,
            bucket.decode_failures.len()
        );

        if accepted < MIN_TRACKS_PER_BUCKET {
            println!(
                "WARNING: Bucket '{}' has fewer than {} accepted tracks (found {})",
                bucket.name, MIN_TRACKS_PER_BUCKET, accepted
            );
        }

        for track in &bucket.tracks {
            if let Err(reasons) = &track.verdict {
                println!(
                    "  Rejected: {:?} - Reasons: {:?} (MFCC: {})",
                    track.path.file_name().unwrap(),
                    reasons,
                    track.mfcc_frames_count
                );
            }
        }
    }
}
