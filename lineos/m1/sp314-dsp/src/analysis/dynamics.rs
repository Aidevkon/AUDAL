// analysis/dynamics.rs — Dynamics feature extraction
// libm only.

/// Crest factor in dB: peak / RMS.
/// High = percussive (drums). Low = compressed.
pub fn crest_factor_db(signal: &[f32]) -> f32 {
    if signal.is_empty() {
        return 10.0;
    }

    let mut peak = 0.0_f32;
    let mut sum_sq = 0.0_f32;

    for &s in signal {
        let abs_s = libm::fabsf(s);
        if abs_s > peak {
            peak = abs_s;
        }
        sum_sq += s * s;
    }

    let rms = libm::sqrtf(sum_sq / signal.len() as f32);
    if rms < 1e-10 {
        return 0.0;
    }

    20.0 * libm::log10f(peak / rms)
}

/// RMS in dBFS.
pub fn rms_db(signal: &[f32]) -> f32 {
    if signal.is_empty() {
        return -144.0;
    }

    let sum_sq: f32 = signal.iter().map(|s| s * s).sum();
    let mean_sq = sum_sq / signal.len() as f32;

    if mean_sq < 1e-30 {
        return -144.0;
    }

    10.0 * libm::log10f(mean_sq)
}

/// Dynamic range in dBFS:
/// 95th percentile - 5th percentile of block RMS values.
/// Block size: DYNAMIC_RANGE_BLOCK_MS at given sample_rate.
pub fn dynamic_range_db(signal: &[f32], sample_rate: u32) -> f32 {
    use super::features::DYNAMIC_RANGE_BLOCK_MS;

    let block_size = (sample_rate as usize * DYNAMIC_RANGE_BLOCK_MS as usize) / 1000;
    if block_size == 0 || signal.len() < block_size {
        return 0.0;
    }

    let mut block_rms: Vec<f32> = signal
        .chunks(block_size)
        .filter(|c| c.len() == block_size)
        .map(rms_db)
        .filter(|&r| r > -144.0)
        .collect();

    if block_rms.is_empty() {
        return 0.0;
    }
    block_rms.sort_by(|a, b| a.total_cmp(b));

    let n = block_rms.len();
    let p95 = block_rms[(n * 95 / 100).min(n - 1)];
    let p5 = block_rms[(n * 5 / 100).min(n - 1)];
    p95 - p5
}

/// Streaming dynamics analyzer.
/// Computes RMS, crest factor, and dynamic range in a single pass.
/// Memory: O(1) except block_rms, which is O(blocks) scalars
/// (this mirrors the offline structure).
pub struct StreamingDynamicsAnalyzer {
    block_size: usize,
    global_sum_sq: f32,
    global_peak: f32,
    global_count: usize,
    block_sum_sq: f32,
    block_count: usize,
    block_rms: Vec<f32>,
}

impl StreamingDynamicsAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        use super::features::DYNAMIC_RANGE_BLOCK_MS;
        let block_size = (sample_rate as usize * DYNAMIC_RANGE_BLOCK_MS as usize) / 1000;
        Self {
            block_size,
            global_sum_sq: 0.0,
            global_peak: 0.0,
            global_count: 0,
            block_sum_sq: 0.0,
            block_count: 0,
            block_rms: Vec::new(),
        }
    }

    pub fn feed_chunk(&mut self, chunk: &[f32]) {
        for &s in chunk {
            // Both offline rms_db and crest_factor_db sum the squares in exact
            // sample order (either via a loop or iter().map().sum() which is a
            // left-to-right fold). Sharing this accumulator is bit-identical to
            // doing it separately.
            let abs_s = libm::fabsf(s);
            if abs_s > self.global_peak {
                self.global_peak = abs_s;
            }
            self.global_sum_sq += s * s;
            self.global_count += 1;

            if self.block_size > 0 {
                self.block_sum_sq += s * s;
                self.block_count += 1;

                if self.block_count == self.block_size {
                    let mean_sq = self.block_sum_sq / self.block_size as f32;
                    let rms = if mean_sq < 1e-30 {
                        -144.0
                    } else {
                        10.0 * libm::log10f(mean_sq)
                    };

                    if rms > -144.0 {
                        self.block_rms.push(rms);
                    }

                    self.block_sum_sq = 0.0;
                    self.block_count = 0;
                }
            }
        }
    }

    pub fn finish(mut self) -> (f32, f32, f32) {
        if self.global_count == 0 {
            return (-144.0, 10.0, 0.0);
        }

        let global_mean_sq = self.global_sum_sq / self.global_count as f32;
        let final_rms = if global_mean_sq < 1e-30 {
            -144.0
        } else {
            10.0 * libm::log10f(global_mean_sq)
        };

        let crest_rms = libm::sqrtf(global_mean_sq);
        let final_crest = if crest_rms < 1e-10 {
            0.0
        } else {
            20.0 * libm::log10f(self.global_peak / crest_rms)
        };

        let final_dyn_rng = if self.block_rms.is_empty() {
            0.0
        } else {
            self.block_rms.sort_by(|a, b| a.total_cmp(b));
            let n = self.block_rms.len();
            let p95 = self.block_rms[(n * 95 / 100).min(n - 1)];
            let p5 = self.block_rms[(n * 5 / 100).min(n - 1)];
            p95 - p5
        };

        (final_rms, final_crest, final_dyn_rng)
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;

    #[test]
    fn streaming_dynamics_matches_offline_reference() {
        use super::super::features::DYNAMIC_RANGE_BLOCK_MS;
        let sr: u32 = 48000;
        let block_ms = DYNAMIC_RANGE_BLOCK_MS as usize;
        let block_size = (sr as usize * block_ms) / 1000;

        let signal_lengths = [
            block_size * 4,
            block_size * 2,
            block_size * 3 + 1500, // non-multiple, tests discard of trailing partial block
            block_size * 1 + 10,
            block_size - 1, // shorter than one block
            1,
            0,
        ];

        let chunk_sizes = [4096, 1024, 4800, 4801, 1];

        let max_len = block_size * 5;
        let mut full_signal = Vec::with_capacity(max_len);
        let mut prng = 12345u32;
        for i in 0..max_len {
            let t = i as f32 / sr as f32;
            let sine = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * t);
            prng = prng.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (prng as f32 / u32::MAX as f32) * 2.0 - 1.0;

            let amp = if i < block_size * 2 {
                0.8
            } else if i < block_size * 3 {
                0.0 // fully-silent stretch (exercises the > -144.0 filter)
            } else {
                0.2
            };

            full_signal.push((sine * 0.5 + noise * 0.5) * amp);
        }

        for &len in &signal_lengths {
            let signal = &full_signal[..len];

            let expected_rms = rms_db(signal);
            let expected_crest = crest_factor_db(signal);
            let expected_dyn = dynamic_range_db(signal, sr);

            for &cs in &chunk_sizes {
                let mut analyzer = StreamingDynamicsAnalyzer::new(sr);
                for chunk in signal.chunks(cs) {
                    analyzer.feed_chunk(chunk);
                }
                let (rms, crest, dyn_rng) = analyzer.finish();

                assert_eq!(
                    rms, expected_rms,
                    "RMS mismatch (len={}, chunk={})",
                    len, cs
                );
                assert_eq!(
                    crest, expected_crest,
                    "Crest mismatch (len={}, chunk={})",
                    len, cs
                );
                assert_eq!(
                    dyn_rng, expected_dyn,
                    "Dynamic Range mismatch (len={}, chunk={})",
                    len, cs
                );
            }

            let mut analyzer = StreamingDynamicsAnalyzer::new(sr);
            let mut pos = 0;
            let mut chunk_idx = 0;
            while pos < signal.len() {
                let cs = chunk_sizes[chunk_idx % chunk_sizes.len()];
                let end = (pos + cs).min(signal.len());
                analyzer.feed_chunk(&signal[pos..end]);
                pos = end;
                chunk_idx += 1;
            }
            let (rms, crest, dyn_rng) = analyzer.finish();

            assert_eq!(rms, expected_rms, "RMS mismatch (len={}, chunk=mixed)", len);
            assert_eq!(
                crest, expected_crest,
                "Crest mismatch (len={}, chunk=mixed)",
                len
            );
            assert_eq!(
                dyn_rng, expected_dyn,
                "Dynamic Range mismatch (len={}, chunk=mixed)",
                len
            );
        }
    }
}
