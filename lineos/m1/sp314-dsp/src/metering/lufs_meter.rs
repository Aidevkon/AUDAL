use crate::metering::filter::KWeightingFilter;
use crate::metering::gating::integrated_lufs;

const HOP_SAMPLES: usize = 4800; // 100ms @ 48kHz
const BLOCK_SAMPLES: usize = 19200; // 400ms @ 48kHz
const HOPS_PER_BLOCK: usize = 4;

/// Streaming EBU R128 / ITU-R BS.1770-4
/// integrated loudness meter.
///
/// Equivalent to the monolithic
/// measure_integrated_lufs() but processes
/// audio in arbitrary-sized chunks rather
/// than requiring the full buffer at once.
///
/// Memory cost: one f32 per 100ms block
/// (10 values/second). For a 2-hour file
/// this is ~72,000 f32s (~281 KB) — four
/// orders of magnitude less than holding
/// the full decoded PCM.
///
/// Usage:
///   let mut meter = LufsMeter::new();
///   for chunk in chunks {
///       meter.process_chunk(&l, &r);
///   }
///   let lufs = meter.finish();
pub struct LufsMeter {
    filter_l: KWeightingFilter,
    filter_r: KWeightingFilter,
    block_mean_squares: Vec<f32>,
    hop_sums: [f32; HOPS_PER_BLOCK],
    hop_idx: usize,
    current_sum: f32,
    sample_count: usize,
}

impl LufsMeter {
    pub fn new() -> Self {
        Self {
            filter_l: KWeightingFilter::new(),
            filter_r: KWeightingFilter::new(),
            block_mean_squares: Vec::new(),
            hop_sums: [0.0; HOPS_PER_BLOCK],
            hop_idx: 0,
            current_sum: 0.0,
            sample_count: 0,
        }
    }

    /// Feed the next chunk of audio.
    /// `left` and `right` must be the same
    /// length. Can be called any number of
    /// times with any buffer size.
    pub fn process_chunk(&mut self, left: &[f32], right: &[f32]) {
        debug_assert_eq!(left.len(), right.len());

        for (&l, &r) in left.iter().zip(right.iter()) {
            let fl = self.filter_l.process(l);
            let fr = self.filter_r.process(r);
            self.current_sum += fl * fl + fr * fr;
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
    }

    /// Finish processing and return
    /// the integrated loudness in LUFS.
    /// Returns None if the audio was too
    /// short to form any gating blocks
    /// (less than 400ms of audio).
    pub fn finish(self) -> Option<f32> {
        if self.block_mean_squares.is_empty() {
            return None;
        }
        Some(integrated_lufs(&self.block_mean_squares))
    }
}

impl Default for LufsMeter {
    fn default() -> Self {
        Self::new()
    }
}
