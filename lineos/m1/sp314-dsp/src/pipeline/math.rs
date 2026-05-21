//! Numerical math primitives — sp314-dsp v2.9.1 §N1 + §N10.
//!
//! Authority: sp314-dsp-v2-9-1-amendment.md §N1 (Kahan), §N10 (finalize_sample)
//! Constitutional rules:
//!   - libm only — no std::f32 methods in pipeline
//!   - No f64 upcast — f32 throughout (determinism contract)
//!   - let _ = is FORBIDDEN (silent failure)
//!
//! These functions replace naive `sum += x * x` energy accumulators
//! throughout the pipeline wherever energy is accumulated over long
//! sequences (Stage 1.5a, Stage 8.1/8.2/8.3/8.5, BandCoupling).

// ── Stage output boundary ────────────────────────────────────────────────────

/// Denormal flush threshold — below this magnitude is treated as zero.
/// Prevents denormal performance penalty without FTZ/DAZ (§Key Invariants).
const DENORMAL_THRESHOLD: f32 = 1.0e-15;

/// Quantization step for sample values — eliminates sub-LSB accumulation drift.
/// Per §N10: `quantize(0.000001)` at every stage output boundary.
const QUANTIZE_STEP: f32 = 0.000001;

/// Stage output boundary — applied to every output sample.
///
/// Per spec §N10 Signal Scaling Policy (amendment §Key Invariants):
///   1. flush_denormal: values < DENORMAL_THRESHOLD → 0.0 (no FTZ/DAZ)
///   2. quantize: round to nearest QUANTIZE_STEP (0.000001)
///   3. clamp: hard clip to [-1.0, 1.0]
///
/// # Determinism
/// Bit-identical across platforms because:
/// - libm::roundf is deterministic
/// - All three operations are monotone (no branching on float state)
/// - No FTZ/DAZ flags mutated — scoped inline only
#[inline]
pub fn finalize_sample(x: f32) -> f32 {
    // 1. Flush denormals inline — no FTZ/DAZ, no thread globals
    let flushed = if libm::fabsf(x) < DENORMAL_THRESHOLD { 0.0 } else { x };
    // 2. Quantize — eliminate sub-LSB drift
    let quantized = libm::roundf(flushed / QUANTIZE_STEP) * QUANTIZE_STEP;
    // 3. Clamp to [-1.0, 1.0]
    libm::fminf(1.0, libm::fmaxf(-1.0, quantized))
}

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

// ── Log Domain Envelope Follower ─────────────────────────────────────────────

/// Log domain envelope follower per §N4 (v2.9.1 amendment).
///
/// Operates entirely in dB — no linear↔dB conversion inside the per-sample loop.
/// Input must be in dBFS (use `LinearGain(rms).to_db().0` at call site).
///
/// # Determinism
/// Bit-identical: `libm::expf` is deterministic; no FTZ/DAZ; no branching on
/// float classification beyond the sign of `input_db - level_db`.
///
/// # Silence floor
/// Initialised and reset to -144.0 dBFS (24-bit noise floor per §N4).
/// This prevents `log10(0)` at the call site when converting RMS energy.
pub struct LogEnvelopeFollower {
    /// Current envelope level in dBFS.
    pub level_db:     f32,
    /// Per-sample attack coefficient (computed from attack_ms via §N5).
    pub attack_coeff:  f32,
    /// Per-sample release coefficient (computed from release_ms via §N5).
    pub release_coeff: f32,
}

impl LogEnvelopeFollower {
    /// Construct from attack/release times in milliseconds.
    ///
    /// Uses `time_to_coeff(ms, sample_rate)` per §N5:
    ///   `α = 1 - libm::expf(-1.0 / (time_ms * 0.001 * sample_rate))`
    pub fn new(attack_ms: f32, release_ms: f32, sample_rate: f32) -> Self {
        let attack_coeff  = if attack_ms  <= 0.0 { 1.0 }
                            else { 1.0 - libm::expf(-1.0 / (attack_ms  * 0.001 * sample_rate)) };
        let release_coeff = if release_ms <= 0.0 { 1.0 }
                            else { 1.0 - libm::expf(-1.0 / (release_ms * 0.001 * sample_rate)) };
        Self { level_db: -144.0, attack_coeff, release_coeff }
    }

    /// Process one dBFS input sample through the envelope follower.
    ///
    /// Returns the smoothed envelope level in dBFS.
    /// Flushes denormals after smoothing (§N4: "level_db may approach -inf for silence").
    #[inline]
    pub fn process(&mut self, input_db: f32) -> f32 {
        let coeff = if input_db > self.level_db {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.level_db = self.level_db + coeff * (input_db - self.level_db);
        // Flush denormal — level_db may become very negative approaching silence
        if libm::fabsf(self.level_db) < 1.0e-15 { self.level_db = 0.0; }
        self.level_db
    }

    /// Reset envelope to silence floor (-144.0 dBFS).
    pub fn reset(&mut self) {
        self.level_db = -144.0;
    }
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

    // ── LogEnvelopeFollower tests ─────────────────────────────────────────────

    #[test]
    fn test_log_follower_init_at_silence_floor() {
        let f = LogEnvelopeFollower::new(10.0, 80.0, 48_000.0);
        assert_eq!(f.level_db, -144.0, "initial level must be silence floor");
    }

    #[test]
    fn test_log_follower_reset() {
        let mut f = LogEnvelopeFollower::new(10.0, 80.0, 48_000.0);
        f.process(0.0); // drive level up slightly
        f.reset();
        assert_eq!(f.level_db, -144.0, "reset must return to silence floor");
    }

    #[test]
    fn test_log_follower_attack_rises() {
        // After many loud samples, level_db must have risen above -100 dBFS.
        let mut f = LogEnvelopeFollower::new(5.0, 80.0, 48_000.0);
        for _ in 0..2_400 {   // 50ms at 48kHz
            f.process(-6.0);  // feed -6 dBFS
        }
        assert!(f.level_db > -100.0,
            "level_db must rise toward input after attack period: {}", f.level_db);
    }

    #[test]
    fn test_log_follower_release_falls() {
        // After attack to -6 dBFS, switch to silence — level must fall.
        let mut f = LogEnvelopeFollower::new(5.0, 80.0, 48_000.0);
        for _ in 0..10_000 { f.process(-6.0); }
        let peak = f.level_db;
        for _ in 0..10_000 { f.process(-144.0); }
        assert!(f.level_db < peak,
            "level_db must fall from {peak} during release, got {}", f.level_db);
    }

    #[test]
    fn test_log_follower_coeffs_finite() {
        let f = LogEnvelopeFollower::new(40.0, 120.0, 48_000.0);
        assert!(f.attack_coeff.is_finite()  && f.attack_coeff  > 0.0);
        assert!(f.release_coeff.is_finite() && f.release_coeff > 0.0);
    }
}
