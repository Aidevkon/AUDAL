// MEA-001: Ground Truth Measurement Script
// Authority: Measurement-First Engineering Rule 1, 2, 3

use chrono::Local;
use lineos_corpus::mfcc::{MfccAnalyzer, FFT_SIZE, N_MFCC};
use m0d::dsp::lazy_reader::LazyAudioReader;
use sp314_dsp::analysis::dynamics::rms_db;
use std::fs;
use std::path::{Path, PathBuf};

const SILENCE_THRESHOLD_DB: f32 = -60.0; // Aligned with signal_health::SILENCE_DBFS
const MIN_TRACKS_PER_BUCKET: usize = 15;

/// Represents the extracted features for a single track (MFCC-only by design, F-016).
#[allow(dead_code)]
struct TrackFeatures {
    path: PathBuf,
    mfcc_mean: [f32; N_MFCC],
    mfcc_variance: [f32; N_MFCC],
}

#[allow(dead_code)]
struct BucketStats {
    name: String,
    tracks: Vec<TrackFeatures>,
}

/// 1. Decode + downmix σε mono.
/// 2. Energy-based frame filtering (silence exclusion).
/// 3. MFCC computation σε chunks των 1024.
fn measure_track(path: &Path) -> Result<(TrackFeatures, Vec<[f32; N_MFCC]>), String> {
    let mut reader = LazyAudioReader::open(path)
        .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    let channels = reader.channels();
    let mut mfcc_analyzer = MfccAnalyzer::new();
    let mut all_frames = Vec::new();

    let mut chunk_buf = vec![0.0_f32; 4096 * channels];
    let mut mono = Vec::new();

    loop {
        let got = reader
            .fill_buffer(&mut chunk_buf)
            .map_err(|e| e.to_string())?;
        if got == 0 {
            break;
        }

        let frames_read = got / channels;
        for i in 0..frames_read {
            let sample = if channels == 1 {
                chunk_buf[i]
            } else if channels == 2 {
                0.5 * (chunk_buf[i * 2] + chunk_buf[i * 2 + 1])
            } else {
                chunk_buf[i * channels]
            };
            mono.push(sample);
        }

        // Process full chunks of FFT_SIZE
        while mono.len() >= FFT_SIZE {
            let chunk: Vec<f32> = mono.drain(..FFT_SIZE).collect();
            let chunk_rms = rms_db(&chunk);
            if chunk_rms >= SILENCE_THRESHOLD_DB {
                let features = mfcc_analyzer.compute(&chunk);
                all_frames.push(features);
            }
        }
    }

    // Process any remaining tail if it happens to be full enough
    while mono.len() >= FFT_SIZE {
        let chunk: Vec<f32> = mono.drain(..FFT_SIZE).collect();
        let chunk_rms = rms_db(&chunk);
        if chunk_rms >= SILENCE_THRESHOLD_DB {
            let features = mfcc_analyzer.compute(&chunk);
            all_frames.push(features);
        }
    }

    if all_frames.is_empty() {
        return Err("No active audio found (all silence)".to_string());
    }

    let n = all_frames.len() as f32;
    let mut mean = [0.0; N_MFCC];
    for frame in &all_frames {
        for i in 0..N_MFCC {
            mean[i] += frame[i];
        }
    }
    for i in 0..N_MFCC {
        mean[i] /= n;
    }

    let mut var = [0.0; N_MFCC];
    for frame in &all_frames {
        for i in 0..N_MFCC {
            let diff = frame[i] - mean[i];
            var[i] += diff * diff;
        }
    }
    for i in 0..N_MFCC {
        var[i] /= n;
    }

    let track = TrackFeatures {
        path: path.to_path_buf(),
        mfcc_mean: mean,
        mfcc_variance: var,
    };

    Ok((track, all_frames))
}

/// 4. Υπολογισμός Global Z-score statistics.
/// Επιστρέφει (global_mean, global_std) ανά MFCC coefficient
/// υπολογισμένο πάνω σε ΟΛΑ τα frames ΟΛΩΝ των buckets.
fn compute_global_stats(all_frames: &[[f32; N_MFCC]]) -> ([f32; N_MFCC], [f32; N_MFCC]) {
    let n = all_frames.len() as f32;
    if n == 0.0 {
        return ([0.0; N_MFCC], [1.0; N_MFCC]);
    }

    let mut mean = [0.0; N_MFCC];
    for frame in all_frames {
        for i in 0..N_MFCC {
            mean[i] += frame[i];
        }
    }
    for i in 0..N_MFCC {
        mean[i] /= n;
    }

    let mut var = [0.0; N_MFCC];
    for frame in all_frames {
        for i in 0..N_MFCC {
            let diff = frame[i] - mean[i];
            var[i] += diff * diff;
        }
    }

    let mut std = [0.0; N_MFCC];
    for i in 0..N_MFCC {
        std[i] = (var[i] / n).sqrt();
    }

    (mean, std)
}

/// 5 & 6. Z-scoring και Aggregate per-bucket (mean + variance).
/// Εφαρμόζει το global Z-score σε κάθε frame του bucket και υπολογίζει
/// το τελικό centroid και variance του genre.
fn compute_bucket_centroid(
    bucket_frames: &[[f32; N_MFCC]],
    global_mean: &[f32; N_MFCC],
    global_std: &[f32; N_MFCC],
) -> ([f32; N_MFCC], [f32; N_MFCC]) {
    let n = bucket_frames.len() as f32;
    if n == 0.0 {
        return ([0.0; N_MFCC], [0.0; N_MFCC]);
    }

    let mut z_frames = Vec::with_capacity(bucket_frames.len());
    for frame in bucket_frames {
        let mut z = [0.0; N_MFCC];
        for i in 0..N_MFCC {
            // Apply z = (x - global_mean) / (global_std + 1e-8) // Αποφυγή division by zero!
            z[i] = (frame[i] - global_mean[i]) / (global_std[i] + 1e-8);
        }
        z_frames.push(z);
    }

    let mut mean = [0.0; N_MFCC];
    for z in &z_frames {
        for i in 0..N_MFCC {
            mean[i] += z[i];
        }
    }
    for i in 0..N_MFCC {
        mean[i] /= n;
    }

    let mut var = [0.0; N_MFCC];
    for z in &z_frames {
        for i in 0..N_MFCC {
            let diff = z[i] - mean[i];
            var[i] += diff * diff;
        }
    }
    for i in 0..N_MFCC {
        var[i] /= n;
    }

    (mean, var)
}

/// 7. Το κύριο orchestration test
#[test]
#[ignore]
fn generate_genre_centroids_code() {
    let base_dir = Path::new("/tmp/genre_references");
    if !base_dir.exists() {
        panic!(
            "Directory {} does not exist. Please place reference tracks there.",
            base_dir.display()
        );
    }

    let mut buckets = Vec::new();
    let mut global_frames = Vec::new();
    let mut bucket_raw_frames: Vec<Vec<[f32; N_MFCC]>> = Vec::new();

    for entry in fs::read_dir(base_dir).expect("Failed to read genre_references dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            let bucket_name = path.file_name().unwrap().to_string_lossy().to_string();
            let mut tracks = Vec::new();
            let mut current_bucket_frames = Vec::new();

            for file_entry in fs::read_dir(&path).expect("Failed to read bucket dir") {
                let file_entry = file_entry.unwrap();
                let file_path = file_entry.path();
                if file_path
                    .extension()
                    .map_or(false, |ext| ext == "wav" || ext == "flac" || ext == "mp3")
                {
                    if let Ok((track, mut frames)) = measure_track(&file_path) {
                        tracks.push(track);
                        global_frames.append(&mut frames.clone());
                        current_bucket_frames.append(&mut frames);
                    } else {
                        eprintln!("Failed to process {}", file_path.display());
                    }
                }
            }

            assert!(
                tracks.len() >= MIN_TRACKS_PER_BUCKET,
                "Bucket {} has too few reference tracks ({}). Must be >= {}.",
                bucket_name,
                tracks.len(),
                MIN_TRACKS_PER_BUCKET
            );

            buckets.push(BucketStats {
                name: bucket_name,
                tracks,
            });
            bucket_raw_frames.push(current_bucket_frames);
        }
    }

    let (g_mean, g_std) = compute_global_stats(&global_frames);

    println!("// MEA-001: Centroids derived from real reference tracks.");
    println!("// Measured on {}", Local::now().to_rfc3339());
    println!("// Generated by lineos/m0/m0-daemon/tests/measure_genre_centroids.rs");
    println!();

    println!("pub const GLOBAL_MFCC_MEAN: [f32; {}] = [", N_MFCC);
    for v in &g_mean {
        println!("    {:.6},", v);
    }
    println!("];");

    println!("pub const GLOBAL_MFCC_STD: [f32; {}] = [", N_MFCC);
    for v in &g_std {
        println!("    {:.6},", v);
    }
    println!("];");
    println!();

    let mut final_centroids = Vec::new();

    for (i, bucket) in buckets.iter().enumerate() {
        let (z_mean, z_var) = compute_bucket_centroid(&bucket_raw_frames[i], &g_mean, &g_std);
        final_centroids.push((bucket.name.clone(), z_mean, z_var));

        let struct_name = bucket.name.to_uppercase();

        println!("pub const {}_MFCC_MEAN: [f32; {}] = [", struct_name, N_MFCC);
        for v in &z_mean {
            println!("    {:.6},", v);
        }
        println!("];");

        println!("pub const {}_MFCC_VAR: [f32; {}] = [", struct_name, N_MFCC);
        for v in &z_var {
            println!("    {:.6},", v);
        }
        println!("];");
        println!();
    }

    println!("// --- DISTANCE DISTRIBUTION DEBUG ---");
    if final_centroids.len() == 2 {
        let (name1, mean1, _) = &final_centroids[0];
        let (name2, mean2, _) = &final_centroids[1];
        let mut sum_sq = 0.0;
        for i in 0..N_MFCC {
            let diff = mean1[i] - mean2[i];
            sum_sq += diff * diff;
        }
        println!(
            "// Inter-class Distance ({} vs {}): {:.6}",
            name1,
            name2,
            sum_sq.sqrt()
        );
    }

    println!("// Intra-class Distances:");
    for (i, bucket) in buckets.iter().enumerate() {
        let (_, c_mean, _) = &final_centroids[i];
        let mut dists = Vec::new();

        for track in &bucket.tracks {
            // Z-score track mean
            let mut z_track = [0.0; N_MFCC];
            for j in 0..N_MFCC {
                z_track[j] = (track.mfcc_mean[j] - g_mean[j]) / (g_std[j] + 1e-8);
            }
            // Compute Euclidean from centroid
            let mut sum_sq = 0.0;
            for j in 0..N_MFCC {
                let diff = z_track[j] - c_mean[j];
                sum_sq += diff * diff;
            }
            dists.push(sum_sq.sqrt());
        }

        // Compute min, max, avg
        if !dists.is_empty() {
            let min = dists.iter().copied().fold(f32::INFINITY, f32::min);
            let max = dists.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let sum: f32 = dists.iter().sum();
            let avg = sum / dists.len() as f32;
            println!(
                "// Bucket {}: Min: {:.6}, Max: {:.6}, Avg: {:.6}",
                bucket.name, min, max, avg
            );
        }
    }

    println!("// --- SANITY CHECK (TRAINING DATA ACCURACY) ---");
    let max_dist = 4.0;
    let min_delta = 0.15;

    // Find centroids
    if final_centroids.len() >= 2 {
        let idm_c = final_centroids.iter().find(|(n, _, _)| n == "idm").unwrap();
        let acoustic_c = final_centroids
            .iter()
            .find(|(n, _, _)| n == "acoustic")
            .unwrap();

        for bucket in &buckets {
            let mut correct = 0;
            let mut incorrect = 0;
            let mut unknown_max = 0;
            let mut unknown_delta = 0;

            let mut is_first_idm = bucket.name == "idm";
            for track in &bucket.tracks {
                if is_first_idm {
                    println!("// FIRST RAW IDM TRACK MEAN for {}:", track.path.display());
                    println!("// [");
                    for v in &track.mfcc_mean {
                        println!("//     {:.6},", v);
                    }
                    println!("// ];");
                    is_first_idm = false;
                }
                let mut z_track = [0.0; N_MFCC];
                for j in 0..N_MFCC {
                    z_track[j] = (track.mfcc_mean[j] - g_mean[j]) / (g_std[j] + 1e-8);
                }

                let mut dist_idm = 0.0;
                let mut dist_acoustic = 0.0;
                for j in 0..N_MFCC {
                    let d_i = z_track[j] - idm_c.1[j];
                    let d_a = z_track[j] - acoustic_c.1[j];
                    dist_idm += d_i * d_i;
                    dist_acoustic += d_a * d_a;
                }
                dist_idm = dist_idm.sqrt();
                dist_acoustic = dist_acoustic.sqrt();

                let (min_dist, predicted_bucket) = if dist_idm < dist_acoustic {
                    (dist_idm, "idm")
                } else {
                    (dist_acoustic, "acoustic")
                };

                if min_dist > max_dist {
                    unknown_max += 1;
                } else if (dist_idm - dist_acoustic).abs() < min_delta {
                    unknown_delta += 1;
                } else {
                    if predicted_bucket == bucket.name {
                        correct += 1;
                    } else {
                        incorrect += 1;
                        println!(
                            "// MISCLASSIFIED ({}->{}): {}",
                            bucket.name,
                            predicted_bucket,
                            track.path.display()
                        );
                    }
                }
            }

            println!("// Bucket {}: Total={}, Correct={}, Incorrect={}, Unknown(Max)={}, Unknown(Delta)={}", 
                bucket.name, bucket.tracks.len(), correct, incorrect, unknown_max, unknown_delta);
        }
    }
}
