use crate::metering::filter::KWeightingFilter;
use crate::metering::gating::{integrated_lufs, BLOCK_SAMPLES, HOP_SAMPLES};

const HOPS_PER_BLOCK: usize = 4;

/// Streaming ITU-R BS.1770-4 multichannel (5.1) loudness meter.
///
/// K-weights each channel independently, sums power with
/// BS.1770-4 surround weights (+1.5 dB for Ls/Rs), excludes LFE.
/// Gating (absolute -70 LUFS + relative -10 LU) reuses
/// gating::integrated_lufs verbatim — only the per-sample
/// channel-power summation differs from the stereo LufsMeter.
///
/// Channel order: SMPTE 5.1 — L, R, C, LFE, Ls, Rs.
///
/// Usage:
///   let mut meter = MultichannelLufsMeter::new();
///   for frame in frames {
///       meter.process_frame(&frame);
///   }
///   let lufs = meter.finish();
pub struct MultichannelLufsMeter {
    filters: [KWeightingFilter; 6],
    block_mean_squares: Vec<f32>,
    hop_sums: [f32; HOPS_PER_BLOCK],
    hop_idx: usize,
    current_sum: f32,
    sample_count: usize,
}

impl MultichannelLufsMeter {
    /// BS.1770-4 channel weights (power domain, SMPTE order).
    /// L=1.0, R=1.0, C=1.0, LFE=0.0 (excluded),
    /// Ls=+1.5 dB = 10^(1.5/10) ≈ 1.4125375,
    /// Rs=+1.5 dB = 10^(1.5/10) ≈ 1.4125375.
    const WEIGHTS: [f32; 6] = [1.0, 1.0, 1.0, 0.0, 1.4125375, 1.4125375];

    pub fn new() -> Self {
        Self {
            filters: core::array::from_fn(|_| KWeightingFilter::new()),
            block_mean_squares: Vec::new(),
            hop_sums: [0.0; HOPS_PER_BLOCK],
            hop_idx: 0,
            current_sum: 0.0,
            sample_count: 0,
        }
    }

    /// Process a single 6-channel SMPTE frame.
    /// Mirrors LufsMeter::process_chunk's per-sample accumulation
    /// exactly — only the power sum differs (weighted multichannel
    /// instead of stereo fl²+fr²).
    #[allow(clippy::needless_range_loop)] // parallel indexing: frame[ch], filters[ch], WEIGHTS[ch]
    pub fn process_frame(&mut self, frame: &[f32; 6]) {
        let mut power_sum = 0.0;
        for ch in 0..6 {
            let fk = self.filters[ch].process(frame[ch]);
            power_sum += Self::WEIGHTS[ch] * fk * fk;
        }

        self.current_sum += power_sum;
        self.sample_count += 1;

        if self.sample_count.is_multiple_of(HOP_SAMPLES) {
            self.hop_sums[self.hop_idx % HOPS_PER_BLOCK] = self.current_sum;
            self.current_sum = 0.0;
            self.hop_idx += 1;

            if self.hop_idx >= HOPS_PER_BLOCK {
                let block_power = self.hop_sums.iter().sum::<f32>() / BLOCK_SAMPLES as f32;
                self.block_mean_squares.push(block_power);
            }
        }
    }

    /// Finish processing and return integrated loudness in LUFS.
    /// Returns None if the audio was too short to form any gating
    /// blocks (less than 400 ms). Reuses gating::integrated_lufs
    /// verbatim — no new gating math.
    pub fn finish(self) -> Option<f32> {
        if self.block_mean_squares.is_empty() {
            return None;
        }
        Some(integrated_lufs(&self.block_mean_squares))
    }
}

impl Default for MultichannelLufsMeter {
    fn default() -> Self {
        Self::new()
    }
}
