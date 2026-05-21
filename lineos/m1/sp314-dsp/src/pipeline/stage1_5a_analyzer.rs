//! Stage 1.5a — Loudness Analyzer — sp314-dsp v2.9 §Stage 1.5 (Analyzer module).
//!
//! Authority:
//!   sp314-dsp-v2-spec.md v2.9 §Stage 1.5 (Stage 1.5a — Analyzer)
//!   sp314-dsp-v2-spec.md v2.9 §Signal Priority System
//!   sp314-dsp-v2-9-1-amendment.md §N1 (Kahan energy accumulation)
//!
//! Constitutional rules:
//!   - libm only — no std::f32 methods
//!   - No f64 upcast — f32 throughout (determinism contract)
//!   - let _ = is FORBIDDEN (silent failure)
//!   - All thresholds are compile-time constants
//!
//! # Architecture
//!
//! Stage 1.5a is a **planning pass** — it runs over the full PCM before
//! any DSP stage executes, producing:
//!   - Short-term LUFS map (block_index → LUFS value, for Segmenter [4b])
//!   - Per-block crest factors (for SignalPriority classification)
//!   - Median crest factor (threshold anchor for SignalPriority thresholds)
//!   - SignalPriority map (block_index → priority, lookup-only downstream)
//!
//! # Short-Term LUFS Note
//!
//! This planning pass uses `−0.691 + 10·log10(kahan_mean_square(block))`
//! without K-weighting biquads. K-weighting is Stage 8 scope (authoritative
//! per k-weighting-spec.md). The planning-pass LUFS is used internally by
//! Stage 1.5b (Segmenter) to detect structural events (> 3 LU difference).
//! It never leaves sp314-dsp and does not affect output PCM or GoldenBlob
//! lufs_integrated (which is Stage 8.1's exclusive authority).
//!
//! # Determinism
//!
//! - `SignalPriority::Body` as initial `prev` per §Determinism Contract.
//! - Median computed by sorting a Vec — sort is deterministic on same input.
//! - Kahan accumulation is bit-identical for same input sequence (§N1).

use alloc::vec::Vec;

use super::math::kahan_mean_square;
use super::signal_priority::{
    classify_signal_priority, SignalPriority, SignalPriorityThresholds,
};

// ── Compile-time constants ────────────────────────────────────────────────────

/// 400ms block at 48kHz (per EBU R128 §3.1, matching AnalysisAccumulator).
const BLOCK_FRAMES_48K: usize = 19_200;

/// Guard: if RMS is below this, crest factor = 0.0 (silence block).
/// Prevents division by near-zero in peak/rms ratio.
const RMS_SILENCE_FLOOR: f32 = 1.0e-9;

/// LUFS floor for the SignalPriority threshold derivation.
/// Blocks below this are excluded from the median crest factor calculation.
/// Matches the EBU R128 absolute gate (-70 LUFS).
const LUFS_ABSOLUTE_GATE: f32 = -70.0;

// ─────────────────────────────────────────────────────────────────────────────

/// Output of Stage 1.5a.
///
/// Per spec §Stage 1.5:
/// ```text
/// Output: LoudnessStats {
///     short_term_lufs_map, median_crest_factor,
///     per_block_crest_factors, signal_priority_map
/// }
/// SignalPriority classification: median_cf ± deltas, hysteresis applied
/// ```
///
/// `short_term_lufs_map`: `(block_index, lufs_value)` — planning-pass LUFS
/// without K-weighting (internal to sp314-dsp, not the GoldenBlob value).
#[derive(Debug)]
pub struct LoudnessStats {
    /// Per-block short-term LUFS proxy (block_index, lufs).
    /// Silence blocks (below absolute gate) are still included with
    /// their raw value — gating is the Segmenter's responsibility.
    pub short_term_lufs_map: Vec<(u64, f32)>,

    /// Median crest factor across all non-silence blocks.
    /// Used as the anchor for SignalPriority thresholds (spec §Signal Priority).
    pub median_crest_factor: f32,

    /// Per-block crest factor (one entry per block, same order as lufs_map).
    pub per_block_crest_factors: Vec<f32>,

    /// Per-block SignalPriority (one entry per block, same order as lufs_map).
    /// Computed via `classify_signal_priority()` with hysteresis.
    /// Lookup-only downstream — computed here, never updated by later stages.
    pub signal_priority_map: Vec<SignalPriority>,
}

/// Stage 1.5a — Loudness Analyzer.
///
/// Stateless after construction: `analyze()` takes the full PCM slice and
/// produces a `LoudnessStats`. Does not mutate pipeline state.
pub struct Stage1_5aAnalyzer {
    #[allow(dead_code)] // used in debug_assert; will be used when wired into master()
    sample_rate: u32,
    channels:    u16,
}

impl Stage1_5aAnalyzer {
    /// Create the analyzer. `sample_rate` must be 48000 (pipeline canonical).
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        debug_assert_eq!(
            sample_rate, 48_000,
            "Stage1_5aAnalyzer: sample_rate must be 48000, got {sample_rate}"
        );
        Self { sample_rate, channels }
    }

    /// Analyze `pcm` and return `LoudnessStats`.
    ///
    /// `pcm` is interleaved at `self.sample_rate` Hz with `self.channels` channels.
    pub fn analyze(&self, pcm: &[f32]) -> LoudnessStats {
        let ch = self.channels as usize;
        // block_samples: total interleaved samples per 400ms block
        let block_samples = BLOCK_FRAMES_48K * ch;

        let num_blocks = if block_samples == 0 || pcm.len() < block_samples {
            0
        } else {
            pcm.len() / block_samples
        };

        let mut short_term_lufs_map     = Vec::with_capacity(num_blocks);
        let mut per_block_crest_factors = Vec::with_capacity(num_blocks);

        // ── Pass 1: per-block crest factor + LUFS proxy ──────────────────────
        for b in 0..num_blocks {
            let start = b * block_samples;
            let end   = start + block_samples;
            let block = &pcm[start..end];

            // Crest factor = peak_abs / rms (per-block, all channels)
            let mut peak_abs: f32 = 0.0;
            for &s in block {
                let a = libm::fabsf(s);
                if a > peak_abs {
                    peak_abs = a;
                }
            }

            let mean_sq = kahan_mean_square(block);
            let rms     = libm::sqrtf(mean_sq);

            let crest_factor = if rms < RMS_SILENCE_FLOOR {
                0.0 // silence block
            } else {
                peak_abs / rms
            };

            // Short-term LUFS proxy (no K-weighting — planning pass)
            let block_lufs = if mean_sq <= 0.0 {
                f32::NEG_INFINITY
            } else {
                -0.691 + 10.0 * libm::log10f(mean_sq)
            };

            short_term_lufs_map.push((b as u64, block_lufs));
            per_block_crest_factors.push(crest_factor);
        }

        // ── Median crest factor ───────────────────────────────────────────────
        // Computed over non-silence blocks (above absolute gate) to prevent
        // silence blocks from pulling the median toward zero, which would
        // collapse all SignalPriority thresholds toward each other.
        let median_crest_factor = {
            let mut valid_cfs: Vec<f32> = per_block_crest_factors
                .iter()
                .zip(short_term_lufs_map.iter())
                .filter(|(_, &(_, lufs))| lufs > LUFS_ABSOLUTE_GATE)
                .map(|(&cf, _)| cf)
                .collect();

            if valid_cfs.is_empty() {
                0.0
            } else {
                // Sort ascending — deterministic on same input
                valid_cfs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
                let mid = valid_cfs.len() / 2;
                if valid_cfs.len() % 2 == 1 {
                    valid_cfs[mid]
                } else {
                    // Even length: average the two middle values
                    (valid_cfs[mid - 1] + valid_cfs[mid]) * 0.5
                }
            }
        };

        // ── SignalPriority thresholds (spec §Signal Priority System) ──────────
        // transient_enter: median_cf + 3.0
        // transient_exit:  median_cf + 2.0
        // noise_enter:     median_cf - 6.0
        // noise_exit:      median_cf - 5.0
        let thresholds = SignalPriorityThresholds {
            transient_enter: median_crest_factor + 3.0,
            transient_exit:  median_crest_factor + 2.0,
            noise_enter:     median_crest_factor - 6.0,
            noise_exit:      median_crest_factor - 5.0,
        };

        // ── Pass 2: SignalPriority map ────────────────────────────────────────
        // Initial prev = Body per §Determinism Contract.
        // Hysteresis: each block's classification depends on the previous block.
        let mut signal_priority_map = Vec::with_capacity(num_blocks);
        let mut prev = SignalPriority::Body; // reset at every pipeline run

        for &cf in &per_block_crest_factors {
            let priority = classify_signal_priority(cf, &thresholds, prev);
            signal_priority_map.push(priority);
            prev = priority;
        }

        LoudnessStats {
            short_term_lufs_map,
            median_crest_factor,
            per_block_crest_factors,
            signal_priority_map,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: generate N frames of a sine wave at given amplitude (stereo).
    fn sine_stereo(frames: usize, amplitude: f32) -> Vec<f32> {
        let sr = 48_000f32;
        let mut out = alloc::vec![0.0f32; frames * 2];
        for f in 0..frames {
            let s = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * f as f32 / sr)
                * amplitude;
            out[f * 2]     = s;
            out[f * 2 + 1] = s;
        }
        out
    }

    #[test]
    fn test_analyzer_silence() {
        // All-zero PCM → all LUFS -inf, all priorities = Body, median_cf = 0.0.
        // Need ≥ 1 block (19200 frames × 2 ch = 38400 samples)
        let pcm = alloc::vec![0.0f32; BLOCK_FRAMES_48K * 2 * 3]; // 3 blocks stereo
        let analyzer = Stage1_5aAnalyzer::new(48_000, 2);
        let stats = analyzer.analyze(&pcm);

        assert_eq!(stats.per_block_crest_factors.len(), 3, "should have 3 blocks");
        assert_eq!(stats.signal_priority_map.len(), 3);
        assert_eq!(stats.short_term_lufs_map.len(), 3);

        // Silence blocks → crest_factor = 0.0
        for &cf in &stats.per_block_crest_factors {
            assert_eq!(cf, 0.0, "silence crest factor must be 0.0");
        }
        // Median on all-silence → 0.0 (no valid blocks above gate)
        assert_eq!(stats.median_crest_factor, 0.0);

        // All priorities → Body (silence → crest_factor=0, thresholds anchor at 0)
        // With median_cf=0, noise_enter=-6, so cf=0 > -6 → stays Body
        for &sp in &stats.signal_priority_map {
            assert_eq!(sp, SignalPriority::Body, "silence should stay Body");
        }

        // LUFS values should all be -infinity or very negative
        for &(_, lufs) in &stats.short_term_lufs_map {
            assert!(
                lufs < -100.0 || lufs == f32::NEG_INFINITY,
                "silence LUFS should be << -100 or -inf, got {lufs}"
            );
        }
    }

    #[test]
    fn test_analyzer_sine() {
        // 2s of 440Hz at -6 dBFS stereo → non-empty maps, finite median, map lengths equal.
        let amplitude = libm::powf(10.0, -6.0 / 20.0);
        let pcm = sine_stereo(48_000 * 2, amplitude);
        let analyzer = Stage1_5aAnalyzer::new(48_000, 2);
        let stats = analyzer.analyze(&pcm);

        // 2s / 0.4s per block = 5 blocks
        assert_eq!(stats.per_block_crest_factors.len(), 5, "2s / 400ms = 5 blocks");
        assert_eq!(stats.signal_priority_map.len(), stats.per_block_crest_factors.len());
        assert_eq!(stats.short_term_lufs_map.len(), stats.per_block_crest_factors.len());

        // Median crest factor must be finite and > 0.0 for a sine wave
        assert!(
            stats.median_crest_factor.is_finite(),
            "median_cf must be finite for a sine wave"
        );
        assert!(
            stats.median_crest_factor > 0.0,
            "median_cf must be > 0 for a sine wave, got {}",
            stats.median_crest_factor
        );

        // Block indices must be sequential (0, 1, 2, 3, 4)
        for (i, &(block_idx, _)) in stats.short_term_lufs_map.iter().enumerate() {
            assert_eq!(block_idx, i as u64, "block_index must be sequential");
        }

        // All LUFS values should be finite (sine is not silence)
        for &(_, lufs) in &stats.short_term_lufs_map {
            assert!(lufs.is_finite(), "sine wave LUFS should be finite, got {lufs}");
        }
    }

    #[test]
    fn test_analyzer_transient_detection() {
        // Pattern: 3 quiet blocks + 1 loud burst + 3 quiet blocks.
        // The burst block should produce at least one Transient priority.
        //
        // We need the burst to exceed median_cf + 3.0:
        //   quiet sine at -30 dBFS: amplitude ≈ 0.032, crest_factor ≈ 1.41 (sine)
        //   burst sine at  0 dBFS:  amplitude ≈ 1.0,   crest_factor ≈ 1.41 (same sine)
        //
        // For a sine, crest_factor = peak/rms = amplitude / (amplitude/sqrt(2)) = sqrt(2) ≈ 1.41
        // regardless of amplitude. So a pure sine won't trigger Transient.
        //
        // Instead, use a block with one spike at full amplitude and silence elsewhere —
        // this gives a very high crest factor.
        let block_size = BLOCK_FRAMES_48K * 2; // stereo samples per block
        let n_quiet = 3usize;
        let quiet_amplitude = libm::powf(10.0, -20.0 / 20.0); // -20 dBFS
        let total_blocks = n_quiet + 1 + n_quiet;
        let mut pcm = alloc::vec![0.0f32; total_blocks * block_size];

        // Fill quiet blocks with low-level sine
        for b in 0..total_blocks {
            if b == n_quiet {
                // Transient block: single spike at full amplitude, rest near-silence
                let spike_idx = b * block_size;
                pcm[spike_idx] = 1.0;
                pcm[spike_idx + 1] = 1.0;
                // Rest of burst block: tiny signal (not silence — keeps RMS nonzero)
                let tiny = 1e-4f32;
                for i in 2..block_size {
                    pcm[b * block_size + i] = tiny * libm::sinf(i as f32 * 0.1);
                }
            } else {
                let sr = 48_000f32;
                for f in 0..BLOCK_FRAMES_48K {
                    let s = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * f as f32 / sr)
                        * quiet_amplitude;
                    pcm[b * block_size + f * 2]     = s;
                    pcm[b * block_size + f * 2 + 1] = s;
                }
            }
        }

        let analyzer = Stage1_5aAnalyzer::new(48_000, 2);
        let stats = analyzer.analyze(&pcm);

        assert_eq!(stats.signal_priority_map.len(), total_blocks);

        // The burst block (index n_quiet) should have a very high crest factor
        let burst_cf = stats.per_block_crest_factors[n_quiet];
        assert!(
            burst_cf > 10.0,
            "burst block crest factor should be >> 10.0, got {burst_cf}"
        );

        // At least one block should be classified as Transient
        let has_transient = stats
            .signal_priority_map
            .iter()
            .any(|&p| p == SignalPriority::Transient);
        assert!(
            has_transient,
            "at least one block should be Transient; priorities: {:?}",
            stats.signal_priority_map
        );
    }
}
