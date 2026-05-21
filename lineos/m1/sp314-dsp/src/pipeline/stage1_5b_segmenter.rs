//! Stage 1.5b — Segmenter — sp314-dsp v2.9 §Stage 1.5 (Segmenter module).
//!
//! Authority: sp314-dsp-v2-spec.md v2.9 §Stage 1.5b
//! Constitutional rules:
//!   - libm only — no std::f32 methods in pipeline
//!   - No f64 upcast — f32 throughout (determinism contract)
//!   - let _ = is FORBIDDEN (silent failure)
//!   - All thresholds are compile-time constants
//!
//! Rule (verbatim): >3 LU difference between current block and running
//! segment mean = structural event → close segment, open new one.
//!
//! Silence blocks (LUFS < −70.0) are excluded from the mean but remain
//! within the segment span. Silence does not trigger structural events.

use alloc::vec::Vec;

use crate::types::units::Decibels;
use super::stage1_5a_analyzer::LoudnessStats;

// ── Compile-time constants ────────────────────────────────────────────────────

/// Structural event threshold per spec §Stage 1.5b: >3 LU = new segment.
const STRUCTURAL_EVENT_LU: f32 = 3.0;

/// EBU R128 absolute gate: blocks below this excluded from segment mean.
const LUFS_ABSOLUTE_GATE: f32 = -70.0;

/// Each block spans 400ms (EBU R128 §3.1, 400ms @ 48kHz).
const BLOCK_DURATION_MS: u64 = 400;

// ─────────────────────────────────────────────────────────────────────────────

/// One contiguous loudness region of the input track.
///
/// Per spec §Stage 1.5 (verbatim):
/// ```text
/// pub struct LoudnessSegment {
///     pub start_ms:       u64,
///     pub end_ms:         u64,
///     pub target_lufs:    f32,
///     pub gain_offset_db: Decibels,  // typed (v2.9)
/// }
/// ```
///
/// `gain_offset_db` is `Decibels(0.0)` — planning placeholder.
/// Stage 6.3 is the authoritative normalization pass (Q1 Option B, approved).
#[derive(Debug, Clone)]
pub struct LoudnessSegment {
    /// Start of segment in ms (block-aligned: `start_block × 400`).
    pub start_ms:       u64,
    /// End of segment in ms (exclusive). Equals next segment's `start_ms`,
    /// or `num_blocks × 400` for the final segment (Q4, approved).
    pub end_ms:         u64,
    /// Mean LUFS of above-gate blocks in this segment.
    /// `f32::NEG_INFINITY` if all blocks are below the gate (Q3, approved).
    pub target_lufs:    f32,
    /// Planning placeholder — `Decibels(0.0)`. Filled by Stage 6.3.
    pub gain_offset_db: Decibels,
}

/// Detect structural events and partition `stats.short_term_lufs_map`
/// into `Vec<LoudnessSegment>`.
///
/// # Guarantees
/// - Empty input → empty `Vec`
/// - Non-empty input → ≥1 segment
/// - Segments are contiguous: `segment[i+1].start_ms == segment[i].end_ms`
/// - Final segment `end_ms` = `num_blocks × 400`
/// - No minimum segment length (spec-faithful per Q2, approved)
pub fn segment(stats: &LoudnessStats, _sample_rate: u32) -> Vec<LoudnessSegment> {
    if stats.short_term_lufs_map.is_empty() {
        return Vec::new();
    }

    let mut segments: Vec<LoudnessSegment> = Vec::new();

    let mut seg_start_block: u64  = 0;
    let mut seg_lufs_sum:    f32  = 0.0;
    let mut seg_valid_count: usize = 0;

    for &(block_idx, lufs) in &stats.short_term_lufs_map {
        let is_valid = lufs > LUFS_ABSOLUTE_GATE && lufs.is_finite();

        let seg_mean: f32 = if seg_valid_count > 0 {
            seg_lufs_sum / seg_valid_count as f32
        } else {
            f32::NEG_INFINITY
        };

        // Structural event: both block and segment mean are valid,
        // and the absolute difference exceeds the 3 LU threshold.
        let structural_event = is_valid
            && seg_mean.is_finite()
            && libm::fabsf(lufs - seg_mean) > STRUCTURAL_EVENT_LU;

        if structural_event {
            // Finalize current segment — it ends at this block's start.
            segments.push(LoudnessSegment {
                start_ms:       seg_start_block * BLOCK_DURATION_MS,
                end_ms:         block_idx * BLOCK_DURATION_MS,
                target_lufs:    seg_lufs_sum / seg_valid_count as f32,
                gain_offset_db: Decibels(0.0),
            });
            // Open new segment starting with this block.
            seg_start_block = block_idx;
            seg_lufs_sum    = if is_valid { lufs } else { 0.0 };
            seg_valid_count = usize::from(is_valid);
        } else {
            if is_valid {
                seg_lufs_sum    += lufs;
                seg_valid_count += 1;
            }
        }
    }

    // Finalize the last (or only) segment.
    let num_blocks  = stats.short_term_lufs_map.len() as u64;
    let target_lufs = if seg_valid_count > 0 {
        seg_lufs_sum / seg_valid_count as f32
    } else {
        f32::NEG_INFINITY
    };
    segments.push(LoudnessSegment {
        start_ms:       seg_start_block * BLOCK_DURATION_MS,
        end_ms:         num_blocks * BLOCK_DURATION_MS,
        target_lufs,
        gain_offset_db: Decibels(0.0),
    });

    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::signal_priority::SignalPriority;

    /// Build a minimal LoudnessStats from a LUFS slice.
    fn stats_from_lufs(lufs_values: &[f32]) -> LoudnessStats {
        let n = lufs_values.len();
        LoudnessStats {
            short_term_lufs_map:     lufs_values.iter()
                .enumerate()
                .map(|(i, &l)| (i as u64, l))
                .collect(),
            median_crest_factor:     0.0,
            per_block_crest_factors: alloc::vec![0.0f32; n],
            signal_priority_map:     alloc::vec![SignalPriority::Body; n],
        }
    }

    #[test]
    fn test_segmenter_empty() {
        let stats = stats_from_lufs(&[]);
        assert!(segment(&stats, 48_000).is_empty(), "empty input → empty Vec");
    }

    #[test]
    fn test_segmenter_single_block() {
        let stats = stats_from_lufs(&[-20.0]);
        let segs = segment(&stats, 48_000);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start_ms, 0);
        assert_eq!(segs[0].end_ms, 400);
        assert!(libm::fabsf(segs[0].target_lufs - (-20.0)) < 0.01);
        assert_eq!(segs[0].gain_offset_db.0, 0.0);
    }

    #[test]
    fn test_segmenter_uniform() {
        // 10 blocks at same LUFS → no structural events → 1 segment.
        let stats = stats_from_lufs(&[-20.0f32; 10]);
        let segs = segment(&stats, 48_000);
        assert_eq!(segs.len(), 1, "uniform LUFS → 1 segment");
        assert_eq!(segs[0].start_ms, 0);
        assert_eq!(segs[0].end_ms, 10 * 400);
    }

    #[test]
    fn test_segmenter_structural_event() {
        // 5 blocks at -20, 5 at -40 → 20 LU gap >> 3 LU → ≥2 segments.
        let mut v = alloc::vec![-20.0f32; 5];
        v.extend_from_slice(&[-40.0f32; 5]);
        let stats = stats_from_lufs(&v);
        let segs = segment(&stats, 48_000);
        assert!(segs.len() >= 2, "quiet→loud → ≥2 segments; got {}", segs.len());
        // Contiguity check
        for i in 1..segs.len() {
            assert_eq!(
                segs[i].start_ms, segs[i - 1].end_ms,
                "gap between segment {} and {}", i - 1, i
            );
        }
        // Final end_ms
        assert_eq!(segs.last().unwrap().end_ms, 10 * 400);
    }

    #[test]
    fn test_segmenter_silence_only() {
        // All blocks below gate → 1 segment with target_lufs = −∞ (Q3).
        let stats = stats_from_lufs(&[-80.0f32; 5]);
        let segs = segment(&stats, 48_000);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].target_lufs, f32::NEG_INFINITY,
            "silence-only segment must have target_lufs = -∞");
        assert_eq!(segs[0].gain_offset_db.0, 0.0);
    }

    #[test]
    fn test_segmenter_block_timing() {
        // 3 blocks within 3 LU → 1 segment spanning 0..1200ms.
        let stats = stats_from_lufs(&[-18.0, -18.5, -19.0]);
        let segs = segment(&stats, 48_000);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start_ms, 0);
        assert_eq!(segs[0].end_ms, 1200, "3 blocks × 400ms = 1200ms");
    }
}
