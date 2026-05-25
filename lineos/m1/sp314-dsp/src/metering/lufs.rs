// src/metering/lufs.rs
// Orchestrates K-Weighting + Block Gating → Integrated LUFS.

use crate::metering::filter::KWeightingFilter;
use crate::metering::gating::{
    integrated_lufs, BLOCK_SAMPLES, HOP_SAMPLES
};

/// Compute Integrated LUFS for a stereo offline buffer.
/// ITU-R BS.1770-4 compliant: K-weighting + absolute + relative gating.
/// Zero allocation in filter path. Allocation only for block collection.
pub fn measure_integrated_lufs(left: &[f32], right: &[f32]) -> f32 {
    debug_assert_eq!(left.len(), right.len());

    let len = left.len();
    if len < BLOCK_SAMPLES {
        return -144.0_f32; // too short for even one block
    }

    // K-Weighting filters (one per channel, stack allocated)
    let mut filter_l = KWeightingFilter::new();
    let mut filter_r = KWeightingFilter::new();

    // Collect per-block mean squares
    let mut block_mean_squares: Vec<f32> = Vec::new();

    let mut block_start = 0;
    while block_start + BLOCK_SAMPLES <= len {
        let mut sum_sq = 0.0_f32;

        for i in block_start..(block_start + BLOCK_SAMPLES) {
            let wl = filter_l.process(left[i]);
            let wr = filter_r.process(right[i]);
            // Stereo: mean of L² + R² per sample
            sum_sq += wl * wl + wr * wr;
        }

        // Mean square for this block (sum of powers, divide by block_size)
        let mean_sq = sum_sq / (BLOCK_SAMPLES as f32);
        block_mean_squares.push(mean_sq);

        block_start += HOP_SAMPLES; // 100ms hop = 75% overlap
    }

    integrated_lufs(&block_mean_squares)
}
