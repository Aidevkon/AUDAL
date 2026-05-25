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

    let mut filter_l = KWeightingFilter::new();
    let mut filter_r = KWeightingFilter::new();

    // Pass 1: Filter linearly and yield per-hop sums
    let filtered_power_iter = left.iter().zip(right.iter()).map(|(&l, &r)| {
        let wl = filter_l.process(l);
        let wr = filter_r.process(r);
        wl * wl + wr * wr
    });

    let mut block_mean_squares = Vec::new();
    let mut hop_sums = [0.0_f32; 4];
    let mut hop_idx = 0;
    
    let mut current_sum = 0.0_f32;
    let mut sample_count = 0;

    // Pass 2: Gate using small fixed-size window
    for power in filtered_power_iter {
        current_sum += power;
        sample_count += 1;

        if sample_count == HOP_SAMPLES {
            hop_sums[hop_idx % 4] = current_sum;
            hop_idx += 1;

            if hop_idx >= 4 {
                let block_power: f32 = hop_sums.iter().sum();
                block_mean_squares.push(block_power / (BLOCK_SAMPLES as f32));
            }

            current_sum = 0.0;
            sample_count = 0;
        }
    }

    integrated_lufs(&block_mean_squares)
}
