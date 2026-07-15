use serde::Serialize;
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

use crate::seekable_provider::ApproximateSeekProvider;

pub fn run_sparse_scout(
    mut reader: impl ApproximateSeekProvider,
    n_samples: usize,
    window_ms: f64,
) -> Result<SparseScoutSummary, SparseScoutError> {
    if n_samples == 0 {
        return Err(SparseScoutError::InvalidParameters(
            "n_samples must be > 0".into(),
        ));
    }

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
