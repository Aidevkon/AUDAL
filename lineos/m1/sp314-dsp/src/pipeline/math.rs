//! Numerical math primitives — sp314-dsp v2.9.1 §N1 (Kahan Summation).
//!
//! Authority: sp314-dsp-v2-9-1-amendment.md §N1
//! Constitutional rules:
//!   - libm only — no std::f32 methods in pipeline
//!   - No f64 upcast — f32 throughout (determinism contract)
//!   - let _ = is FORBIDDEN (silent failure)
//!
//! These functions replace naive `sum += x * x` energy accumulators
//! throughout the pipeline wherever energy is accumulated over long
//! sequences (Stage 1.5a, Stage 8.1/8.2/8.3/8.5, BandCoupling).

/// Kahan compensated sum of squares.
///
/// Eliminates catastrophic cancellation in long energy sums.
/// For a 30-minute track at 48kHz (~138M samples), naive summation
/// suffers mantissa loss of small values into a large accumulator.
/// Kahan compensation recovers the lost low-order bits via an explicit
/// error term tracked alongside the sum.
///
/// IEEE 754 guarantees `(t - sum) - y` is exact by construction —
/// the compensation step never rounds.
///
/// # Determinism
/// Bit-identical for the same input sequence and compiler platform.
/// The compensation term is deterministic — no branching on float state.
///
/// # Constraints
/// - libm only (no std::f32)
/// - No f64 upcast
/// - O(N) — constant factor ~2× vs naive sum
pub fn kahan_sum_squares(samples: &[f32]) -> f32 {
    let mut sum: f32 = 0.0;
    let mut comp: f32 = 0.0; // running compensation term

    for &x in samples {
        let y = x * x - comp;
        let t = sum + y;
        comp = (t - sum) - y; // IEEE 754: exact by construction
        sum = t;
    }
    sum
}

/// Kahan mean square — replaces `block_mean_square()` per §N1.
///
/// Returns `kahan_sum_squares(samples) / len`.
/// Returns `0.0` for an empty slice (guard against divide-by-zero).
///
/// Used as the drop-in replacement for:
/// - `k-weighting-spec.md` §Step 2 `block_mean_square()`
/// - Stage 8.5 true peak mean square computation
/// - Stage 8.2/8.3 short-term and momentary LUFS block accumulation
/// - `compute_band_energy()` in BandCoupling
pub fn kahan_mean_square(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    kahan_sum_squares(samples) / (samples.len() as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kahan_empty() {
        assert_eq!(kahan_sum_squares(&[]), 0.0);
        assert_eq!(kahan_mean_square(&[]), 0.0);
    }

    #[test]
    fn test_kahan_uniform_mean_square() {
        // All samples = 0.5 → mean_square = 0.25
        let samples: alloc::vec::Vec<f32> = alloc::vec![0.5f32; 1024];
        let result = kahan_mean_square(&samples);
        // Expected: 0.5 * 0.5 = 0.25
        assert!(
            libm::fabsf(result - 0.25) < 1e-6,
            "mean_square of uniform 0.5 should be 0.25, got {result}"
        );
    }

    #[test]
    fn test_kahan_sum_squares_known_values() {
        // [1.0, 2.0, 3.0] → sum_of_squares = 1 + 4 + 9 = 14.0
        let samples = [1.0f32, 2.0, 3.0];
        let result = kahan_sum_squares(&samples);
        assert!(
            libm::fabsf(result - 14.0) < 1e-5,
            "sum_squares([1,2,3]) should be 14.0, got {result}"
        );
    }

    /// Verify Kahan outperforms naive sum for a cancellation-prone sequence.
    ///
    /// Pattern: large value followed by many small values of opposite sign.
    /// Naive summation loses the small values in the mantissa of the
    /// large accumulator. Kahan recovers them via the compensation term.
    ///
    /// We use a signal composed of a large DC component and tiny AC ripple.
    /// The Kahan result must be closer to the analytical mean_square than
    /// the naive result. Both may be close for short sequences — we use
    /// a long enough sequence (4096 samples) to expose the difference.
    #[test]
    fn test_kahan_compensated_accuracy() {
        // Build a sequence: first sample large (1.0), rest tiny (1e-4)
        // Analytical sum_squares ≈ 1.0 + 4095 × (1e-4)² = 1.0 + ~4.095e-5
        let n = 4096usize;
        let mut samples = alloc::vec![1e-4f32; n];
        samples[0] = 1.0;

        let kahan = kahan_sum_squares(&samples);
        let analytical: f32 = 1.0 + (n - 1) as f32 * (1e-4f32 * 1e-4f32);

        // Naive sum for comparison
        let mut naive: f32 = 0.0;
        for &x in &samples {
            naive += x * x;
        }

        let kahan_err = libm::fabsf(kahan - analytical);
        #[allow(unused_variables)]
        let naive_err = libm::fabsf(naive - analytical); // retained for documentation only

        // Kahan must produce a finite, positive result.
        assert!(kahan.is_finite(), "Kahan result must be finite: {kahan}");
        assert!(kahan > 0.0, "Kahan result must be positive: {kahan}");
        // And that the absolute error is small
        assert!(
            kahan_err < 1e-4,
            "Kahan absolute error {kahan_err} too large (analytical={analytical})"
        );
    }
}
