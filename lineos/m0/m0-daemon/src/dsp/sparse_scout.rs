use crate::dsp::lazy_reader::LazyAudioReader;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct ScoutBucket {
    pub timestamp_sec: f32,
    pub peak_linear: f32,
    pub rms_linear: f32,
}

/// NOTE: channel handling is naive interleaved-energy, NOT ITU-R
/// BS.1770-4 aware. On >2-channel (5.1/surround) input, LFE is
/// included un-weighted and surround channels are not weighted
/// ×1.5 per spec — results are numerically valid but not loudness-
/// standard-compliant on multichannel input. See vision doc §11.6.
#[derive(Debug, Clone, Serialize)]
pub struct SparseScoutSummary {
    pub buckets: Vec<ScoutBucket>,

    /// This is the maximum peak observed across the n_samples sparse windows —
    /// NOT a guarantee of the file's absolute peak. A transient outside all
    /// sampled windows will be missed.
    pub sampled_peak_linear: f32,

    /// Quadratic mean (RMS of per-window energy) across sampled windows, NOT
    /// a simple arithmetic average — the two differ whenever window loudness
    /// varies (QM-AM inequality). Same accumulation pattern as signal_health.rs.
    pub mean_rms: f32,

    pub total_seeks_done: usize,
}

#[derive(Debug)]
pub enum SparseScoutError {
    InvalidParameters(String),
    UnknownDuration,
    Symphonia(String),
    AllSeeksFailed,
}

pub fn run_sparse_scout(
    path: &Path,
    n_samples: usize,
    window_ms: f64,
) -> Result<SparseScoutSummary, SparseScoutError> {
    if n_samples == 0 {
        return Err(SparseScoutError::InvalidParameters(
            "n_samples must be > 0".into(),
        ));
    }

    let mut reader =
        LazyAudioReader::open(path).map_err(|e| SparseScoutError::Symphonia(e.to_string()))?;

    let total_frames = match reader.total_frames_hint() {
        Some(frames) => frames,
        None => return Err(SparseScoutError::UnknownDuration),
    };

    let sr = reader.sample_rate();
    let channels = reader.channels();
    let dur_secs = total_frames as f64 / sr as f64;

    let window_frames = (sr as f64 * window_ms / 1000.0).round() as usize;
    if window_frames == 0 {
        return Err(SparseScoutError::InvalidParameters(
            "window_ms is too small".into(),
        ));
    }

    let mut buffer = vec![0.0_f32; window_frames * channels];
    let mut buckets = Vec::with_capacity(n_samples);

    let mut total_sum_sq = 0.0_f64;
    let mut total_samples_processed = 0_usize;
    let mut global_peak = 0.0_f32;
    let mut total_seeks_done = 0_usize;

    for i in 0..n_samples {
        let offset_sec = i as f64 * (dur_secs / n_samples as f64);
        let target = Duration::from_secs_f64(offset_sec);

        let actual_duration = match reader.seek_approximate(target) {
            Ok(d) => d,
            Err(_) => continue, // skip bad seeks
        };

        let filled_frames = match reader.fill_buffer(&mut buffer) {
            Ok(f) => f,
            Err(_) => continue, // skip on read error
        };

        if filled_frames == 0 {
            continue;
        }

        let mut local_max = 0.0_f32;
        let mut sum_sq = 0.0_f64;

        let valid_samples = filled_frames * channels;
        for &sample in buffer[..valid_samples].iter() {
            let val = sample as f64;
            sum_sq += val * val;

            let abs_val = sample.abs();
            if abs_val > local_max {
                local_max = abs_val;
            }
        }

        let local_rms = (sum_sq / valid_samples as f64).sqrt() as f32;

        total_sum_sq += sum_sq;
        total_samples_processed += valid_samples;

        if local_max > global_peak {
            global_peak = local_max;
        }

        buckets.push(ScoutBucket {
            timestamp_sec: actual_duration.as_secs_f64() as f32,
            peak_linear: local_max,
            rms_linear: local_rms,
        });

        total_seeks_done += 1;
    }

    if total_seeks_done == 0 {
        return Err(SparseScoutError::AllSeeksFailed);
    }

    let mean_rms = (total_sum_sq / total_samples_processed as f64).sqrt() as f32;

    Ok(SparseScoutSummary {
        buckets,
        sampled_peak_linear: global_peak,
        mean_rms,
        total_seeks_done,
    })
}

#[cfg(test)]
mod tests {
    // We import what will eventually be the implementation
    use super::*;
    use std::fs;
    use std::path::Path;

    // Helper to generate a deterministic sine wave for testing
    fn write_test_wav(path: &Path, sr: u32, dur_secs: f32, freq: f32, amp: f32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (sr as f32 * dur_secs) as usize;
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * amp;
            w.write_sample(s).unwrap(); // L
            w.write_sample(s).unwrap(); // R
        }
        w.finalize().unwrap();
    }

    #[test]
    fn test_known_synthetic_signal_accuracy() {
        let path = Path::new("/tmp/sparse_scout_test_accuracy.wav");
        // 500Hz sine ensures a 10ms window contains exactly 5 full cycles (at 48kHz).
        write_test_wav(path, 48000, 60.0, 500.0, 0.5);

        let result = run_sparse_scout(path, 50, 10.0).expect("scout should succeed");

        // Assert peak is exactly 0.5 (with strict floating point tolerance)
        assert!(
            (result.sampled_peak_linear - 0.5).abs() < 1e-4,
            "expected sampled_peak_linear ≈ 0.5, got {}",
            result.sampled_peak_linear
        );

        // Assert mean RMS is exactly 0.5 / sqrt(2) ≈ 0.353553
        let expected_rms = 0.5 / 2.0_f32.sqrt();
        assert!(
            (result.mean_rms - expected_rms).abs() < 1e-4,
            "expected mean_rms ≈ {}, got {}",
            expected_rms,
            result.mean_rms
        );

        fs::remove_file(path).ok();
    }

    #[test]
    fn test_short_file_edge_case() {
        let path = Path::new("/tmp/sparse_scout_test_short.wav");
        write_test_wav(path, 48000, 2.0, 500.0, 0.3); // Only 2 seconds long

        // Ask for 300 samples (redundant coverage, will overlap heavily)
        let result =
            run_sparse_scout(path, 300, 10.0).expect("scout should not crash on short file");

        // Total seeks should be properly bounded/recorded
        assert!(
            result.total_seeks_done <= 300,
            "seeks should be bounded by n_samples, got {}",
            result.total_seeks_done
        );

        // Assert no bucket read trailing garbage (which would drop the rms to near 0 or NaN)
        // Since the signal is a 0.3 amplitude sine, RMS should be ~ 0.212
        for bucket in &result.buckets {
            assert!(
                bucket.rms_linear > 0.20 && !bucket.rms_linear.is_nan(),
                "Bucket RMS {} is invalid, likely read past filled_count into trailing zeros",
                bucket.rms_linear
            );
        }

        fs::remove_file(path).ok();
    }

    #[test]
    fn test_transient_spike_outside_sampled_windows() {
        let path = Path::new("/tmp/sparse_scout_test_transient.wav");
        let sr = 48000;
        let dur_secs = 10.0;
        let n_samples = 5;
        let window_ms = 10.0;

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (sr as f32 * dur_secs) as usize;

        // Math Proof:
        // Spacing: 10.0s / 5 = 2.0s
        // Offsets: 0.0s, 2.0s, 4.0s, 6.0s, 8.0s
        // window_ms: 10.0ms (0.01s).
        // Transient at 3.0s is 0.99s away from window 2 (ends at 2.01s)
        // and 1.0s away from window 3 (starts at 4.0s).
        let transient_frame = (sr as f32 * 3.0) as usize;

        for i in 0..n {
            let t = i as f32 / sr as f32;
            let mut s = (2.0 * std::f32::consts::PI * 500.0 * t).sin() * 0.1;

            // The spike is 100 frames (2.08ms at 48kHz). This simulates a sharp, realistic
            // digital glitch or snare transient. It is deliberately short to ensure it fits
            // cleanly into the 1.0s un-sampled gap without coming dangerously close to
            // any window boundaries, ensuring a pure negative-control test.
            if i >= transient_frame && i < transient_frame + 100 {
                s = 1.0; // The hidden transient spike
            }

            w.write_sample(s).unwrap();
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();

        let result = run_sparse_scout(path, n_samples, window_ms).expect("scout should succeed");

        assert!(result.sampled_peak_linear < 0.9,
            "Sampling should miss the transient spike at 3.0s. Proves sampled_peak_linear limitation. Got {}",
            result.sampled_peak_linear);

        fs::remove_file(path).ok();
    }

    #[test]
    fn test_quadratic_mean_differs_from_arithmetic_mean_under_varying_loudness() {
        let path = Path::new("/tmp/sparse_scout_test_varying.wav");
        let sr = 48000;
        let dur_secs = 8.0;
        let n_samples = 4;
        let window_ms = 10.0;

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (sr as f32 * dur_secs) as usize;

        // Math Proof (Revised):
        // Offsets are deterministic: 0.0, 2.0, 4.0, 6.0s.
        // Symphonia's `seek_approximate` on WAV can snap slightly early (e.g. 1.992s instead of 2.0s).
        // If segments changed exactly at 2.0s, the 10ms window starting at 1.992s would straddle
        // the boundary and mix Loud+Silence, breaking the pure math.
        // To fix this, we shift the segments so boundaries are at 1.0s, 3.0s, 5.0s, 7.0s.
        // Offset 0.0s is cleanly inside Loud [0.0, 1.0)
        // Offset 2.0s is cleanly inside Silence [1.0, 3.0)
        // Offset 4.0s is cleanly inside Loud [3.0, 5.0)
        // Offset 6.0s is cleanly inside Silence [5.0, 7.0)
        for i in 0..n {
            let time_sec = i as f32 / sr as f32;
            let segment_idx = ((time_sec + 1.0) / 2.0) as usize;
            let s = if segment_idx % 2 == 0 { 1.0 } else { 0.0 };
            w.write_sample(s).unwrap();
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();

        let result = run_sparse_scout(path, n_samples, window_ms).expect("scout should succeed");

        // Math Proof:
        // Offsets at ~0.0s (Loud), ~2.0s (Silence), ~4.0s (Loud), ~6.0s (Silence).
        // Bucket RMS values: [1.0, 0.0, 1.0, 0.0]
        // Arithmetic Mean = (1.0 + 0.0 + 1.0 + 0.0) / 4 = 0.5
        // Quadratic Mean = sqrt((1.0^2 + 0.0^2 + 1.0^2 + 0.0^2) / 4) = sqrt(0.5) ≈ 0.707106

        // Clean up the debug patch just in case

        let expected_qm = 0.5_f32.sqrt();
        assert!(
            (result.mean_rms - expected_qm).abs() < 1e-4,
            "mean_rms MUST be Quadratic Mean (~0.7071), NOT Arithmetic Mean (0.5). Got {}",
            result.mean_rms
        );

        // Explicitly assert it's NOT the arithmetic mean just to be sure
        assert!(
            (result.mean_rms - 0.5).abs() > 0.1,
            "mean_rms erroneously returned the Arithmetic Mean (0.5)"
        );

        fs::remove_file(path).ok();
    }
    #[test]
    fn test_all_seeks_fail_returns_error_not_zero() {
        let path = Path::new("/tmp/sparse_scout_test_all_fail.wav");

        // Write a valid but extremely tiny WAV file, only 2 samples (1 frame)
        write_test_wav(path, 48000, 0.0001, 500.0, 0.1);

        // Ask for 5 samples with a huge window.
        // Because the file is extremely short, `seek_approximate` on offsets like 2.0s or 4.0s will definitely fail
        // or return 0 filled_frames immediately.
        // Actually, to guarantee all seeks fail/yield nothing, we can ask for a large offset manually,
        // but `run_sparse_scout` calculates offsets dynamically.
        // If the file is 1 frame, duration is ~0.0s. Offsets will all be 0.0s.
        // But the first seek at 0.0 might succeed and read 1 frame! We want 0 frames to be processed.
        // If window is 10ms but file is 1 frame, fill_buffer will return 1 frame.
        // So that would be 1 successful seek (although tiny).

        // Wait, how can we force all seeks to fail or return 0?
        // We can create an invalid path to a file that doesn't exist?
        // No, `LazyAudioReader::open` will fail immediately with Symphonia error, not `AllSeeksFailed`.
        // We want the loops to run, but `fill_buffer` or `seek` to fail.
        // What if we pass a window_ms so small that `window_frames` is 0?
        // It returns `InvalidParameters`.
        // A simple way to trigger `AllSeeksFailed` without mocking:
        // We could write a completely silent file and truncate it abruptly?

        // Let's use a zero-length WAV file.
        // Wait, if total_frames is 0, duration is 0, offset is 0.
        // But `Symphonia` will probably read 0 frames on the very first try, meaning `filled_frames == 0` -> continue.
        // And then total_seeks_done remains 0.
        // Let's create an empty WAV.
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let w = hound::WavWriter::create(path, spec).unwrap();
        w.finalize().unwrap();

        let result = run_sparse_scout(path, 5, 10.0);

        assert!(
            matches!(result, Err(SparseScoutError::AllSeeksFailed)),
            "Expected AllSeeksFailed when file has no readable data, got {:?}",
            result
        );

        fs::remove_file(path).ok();
    }
}
