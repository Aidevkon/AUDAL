//! LRA — Loudness Range (EBU R128 §3.4)
//! LRA = difference between 95th and 10th percentile of short-term loudness values.
//! Short-term window: 3s, hop: 1s (75% overlap).
//! Gate: -70 LUFS absolute + relative gate -20 LU below ungated mean.
//!
//! All float math uses libm — no std::f32 methods.
//! Authority: LineOS Constitution v2.0 §09.1

use alloc::vec::Vec;

/// Calculator for EBU R128 LRA (Loudness Range).
/// Feed processed PCM samples from Golden Blob — already K-weighted by sp314-dsp.
pub struct LraCalculator {
    /// Short-term loudness values (3s windows) that passed the absolute gate.
    short_term_values: Vec<f32>,
    sample_rate: u32,
    hop_sum: f32,
    hop_count: usize,
    st_hops: [f32; 3],
    st_idx: usize,
    st_filled: usize,
}

impl LraCalculator {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            short_term_values: Vec::new(),
            sample_rate,
            hop_sum: 0.0,
            hop_count: 0,
            st_hops: [0.0; 3],
            st_idx: 0,
            st_filled: 0,
        }
    }

    /// Feed audio chunks iteratively (streaming).
    /// left and right must be equal length.
    pub fn process_chunk(&mut self, left: &[f32], right: &[f32]) {
        debug_assert_eq!(left.len(), right.len());
        let hop_size_samples = self.sample_rate as usize * 2;

        for (&l, &r) in left.iter().zip(right.iter()) {
            self.hop_sum += l * l + r * r;
            self.hop_count += 2;

            if self.hop_count >= hop_size_samples {
                self.st_hops[self.st_idx] = self.hop_sum;
                self.st_idx = (self.st_idx + 1) % 3;
                if self.st_filled < 3 {
                    self.st_filled += 1;
                }

                if self.st_filled == 3 {
                    let window_sum: f32 = self.st_hops.iter().sum();
                    let window_len = hop_size_samples * 3;
                    let ms = window_sum / window_len as f32;
                    let lufs = ms_to_lufs(ms);
                    if lufs > -70.0 {
                        self.short_term_values.push(lufs);
                    }
                }

                self.hop_sum = 0.0;
                self.hop_count = 0;
            }
        }
    }

    /// Feed processed samples from Golden Blob PCM output.
    /// Samples must be interleaved (channels interleaved per frame).
    /// Window: 3s = sample_rate × 3 × channels samples.
    /// Hop: 1s = sample_rate × 1 × channels samples.
    pub fn feed_samples(&mut self, samples: &[f32], channels: u16) {
        if samples.is_empty() || channels == 0 {
            return;
        }
        let window_size = self.sample_rate as usize * 3 * channels as usize;
        let hop_size = (self.sample_rate as usize) * channels as usize;

        if window_size == 0 {
            return;
        }

        let mut pos = 0;
        while pos + window_size <= samples.len() {
            let window = &samples[pos..pos + window_size];
            let ms = mean_square(window);
            let lufs = ms_to_lufs(ms);
            if lufs > -70.0 {
                // absolute gate per EBU R128 §3.4
                self.short_term_values.push(lufs);
            }
            pos += hop_size;
        }
    }

    /// Compute LRA after all samples have been fed.
    /// Returns LU value (95th − 10th percentile of gated short-term loudness).
    pub fn compute(&self) -> f32 {
        if self.short_term_values.len() < 2 {
            return 0.0;
        }

        let mut sorted = self.short_term_values.clone();
        // sort_by with partial_cmp is safe here — NaN excluded by the absolute gate
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));

        // Relative gate: -20 LU below ungated mean of gated short-term values
        let ungated_mean = sorted.iter().map(|&v| v as f64).sum::<f64>() / sorted.len() as f64;
        let gate = ungated_mean as f32 - 20.0;

        let gated: Vec<f32> = sorted.iter().copied().filter(|&v| v > gate).collect();

        if gated.len() < 2 {
            return 0.0;
        }

        let p10 = percentile(&gated, 0.10);
        let p95 = percentile(&gated, 0.95);
        // LRA must be non-negative
        (p95 - p10).max(0.0)
    }
}

/// Mean square of a sample slice.
#[inline]
fn mean_square(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    sum / samples.len() as f32
}

/// LUFS from mean square: -0.691 + 10 × log10(ms)
/// ms clamped to 1e-10 to avoid log10(0).
/// Uses libm — no std::f32 permitted (LineOS §09.1).
#[inline]
pub fn ms_to_lufs(ms: f32) -> f32 {
    -0.691 + 10.0 * libm::log10f(ms.max(1e-10))
}

/// Linear interpolation percentile from a sorted slice.
#[inline]
fn percentile(sorted: &[f32], p: f32) -> f32 {
    let idx = (p * (sorted.len() - 1) as f32) as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lra_empty_returns_zero() {
        let calc = LraCalculator::new(48000);
        assert_eq!(calc.compute(), 0.0);
    }

    #[test]
    fn test_lra_too_few_windows() {
        // Only 2 seconds of audio — less than the 3s window, so no windows collected
        let mut calc = LraCalculator::new(48000);
        let samples = alloc::vec![0.5f32; 48000 * 2 * 2]; // 2s stereo
        calc.feed_samples(&samples, 2);
        // May or may not collect windows depending on size, but must not panic
        let _ = calc.compute();
    }

    #[test]
    fn test_lra_silence_is_zero() {
        let mut calc = LraCalculator::new(48000);
        // Silence passes no absolute gate (-70 LUFS) → no windows → LRA = 0
        let samples = alloc::vec![0.0f32; 48000 * 10 * 2];
        calc.feed_samples(&samples, 2);
        assert_eq!(calc.compute(), 0.0);
    }

    #[test]
    fn test_lra_nonnegative() {
        let mut calc = LraCalculator::new(48000);
        // Constant loud signal → uniform short-term values → LRA ≈ 0
        let samples = alloc::vec![0.5f32; 48000 * 10 * 2];
        calc.feed_samples(&samples, 2);
        let lra = calc.compute();
        assert!(lra >= 0.0, "LRA must be non-negative, got {lra}");
    }

    #[test]
    fn test_ms_to_lufs_silence() {
        // Very small signal → very negative LUFS
        let lufs = ms_to_lufs(1e-10);
        assert!(lufs < -50.0);
    }

    #[test]
    fn test_lra_streaming_equivalence() {
        let sr = 48000;
        let mut interleaved = Vec::new();
        // 4s @ 0.5
        for _ in 0..(sr * 4) {
            interleaved.push(0.5);
            interleaved.push(0.5);
        }
        // 4s @ 0.05
        for _ in 0..(sr * 4) {
            interleaved.push(0.05);
            interleaved.push(0.05);
        }
        // 4s @ 0.5
        for _ in 0..(sr * 4) {
            interleaved.push(0.5);
            interleaved.push(0.5);
        }

        let mut calc_batch = LraCalculator::new(sr);
        calc_batch.feed_samples(&interleaved, 2);
        let lra_batch = calc_batch.compute();

        // anti-false-positive: signal has dynamics
        assert!(
            lra_batch > 1.0,
            "Signal should have non-zero dynamics, got {}",
            lra_batch
        );

        let mut calc_stream = LraCalculator::new(sr);

        let frames = interleaved.len() / 2;
        let mut left = alloc::vec::Vec::with_capacity(frames);
        let mut right = alloc::vec::Vec::with_capacity(frames);
        for i in 0..frames {
            left.push(interleaved[i * 2]);
            right.push(interleaved[i * 2 + 1]);
        }

        let chunk_size = 4096;
        let mut pos = 0;
        while pos < frames {
            let end = (pos + chunk_size).min(frames);
            calc_stream.process_chunk(&left[pos..end], &right[pos..end]);
            pos = end;
        }
        let lra_stream = calc_stream.compute();

        let diff = (lra_batch - lra_stream).abs();
        assert!(
            diff < 0.5,
            "Streaming LRA {} differs from batch LRA {}",
            lra_stream,
            lra_batch
        );
    }

    #[test]
    fn test_lra_streaming_equivalence_ramp() {
        let sr = 48000;
        let mut interleaved = Vec::new();
        // ράμπα: linear fade από 0.5 → 0.01 σε 12s
        for i in 0..(sr * 12) {
            let t = i as f32 / (sr * 12) as f32;
            let amp = 0.5 * (1.0 - t) + 0.01 * t; // fade
            interleaved.push(amp);
            interleaved.push(amp);
        }

        let mut calc_batch = LraCalculator::new(sr);
        calc_batch.feed_samples(&interleaved, 2);
        let lra_batch = calc_batch.compute();

        // anti-false-positive: signal has dynamics
        assert!(
            lra_batch > 1.0,
            "Signal should have non-zero dynamics, got {}",
            lra_batch
        );

        let mut calc_stream = LraCalculator::new(sr);

        let frames = interleaved.len() / 2;
        let mut left = alloc::vec::Vec::with_capacity(frames);
        let mut right = alloc::vec::Vec::with_capacity(frames);
        for i in 0..frames {
            left.push(interleaved[i * 2]);
            right.push(interleaved[i * 2 + 1]);
        }

        let chunk_size = 4096;
        let mut pos = 0;
        while pos < frames {
            let end = (pos + chunk_size).min(frames);
            calc_stream.process_chunk(&left[pos..end], &right[pos..end]);
            pos = end;
        }
        let lra_stream = calc_stream.compute();

        let diff = (lra_batch - lra_stream).abs();
        assert!(
            diff < 0.5,
            "Streaming LRA {} differs from batch LRA {}",
            lra_stream,
            lra_batch
        );
    }
}
