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
    sample_rate:       u32,
}

impl LraCalculator {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            short_term_values: Vec::new(),
            sample_rate,
        }
    }

    /// Feed processed samples from Golden Blob PCM output.
    /// Samples must be interleaved (channels interleaved per frame).
    /// Window: 3s = sample_rate × 3 × channels samples.
    /// Hop: 1s = sample_rate × 1 × channels samples.
    pub fn feed_samples(&mut self, samples: &[f32], channels: u16) {
        if samples.is_empty() || channels == 0 { return; }
        let window_size = self.sample_rate as usize * 3 * channels as usize;
        let hop_size    = (self.sample_rate as usize) * channels as usize;

        if window_size == 0 { return; }

        let mut pos = 0;
        while pos + window_size <= samples.len() {
            let window = &samples[pos..pos + window_size];
            let ms     = mean_square(window);
            let lufs   = ms_to_lufs(ms);
            if lufs > -70.0 {  // absolute gate per EBU R128 §3.4
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
        let ungated_mean = sorted.iter().map(|&v| v as f64).sum::<f64>()
            / sorted.len() as f64;
        let gate = ungated_mean as f32 - 20.0;

        let gated: Vec<f32> = sorted.iter()
            .copied()
            .filter(|&v| v > gate)
            .collect();

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
    if samples.is_empty() { return 0.0; }
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
}
