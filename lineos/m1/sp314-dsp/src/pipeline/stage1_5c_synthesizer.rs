//! Stage 1.5c — Synthesizer — sp314-dsp v2.9 §Stage 1.5 (Synthesizer module).
//!
//! Authority: sp314-dsp-v2-spec.md v2.9 §Stage 1.5c + §Latency Compensation
//! Constitutional rules:
//!   - libm only — no std::f32 methods in pipeline
//!   - No f64 upcast — f32 throughout (determinism contract)
//!   - let _ = is FORBIDDEN (silent failure)
//!   - All thresholds are compile-time constants
//!
//! # Role in Planning Pass
//!
//! The Synthesizer is the final stage of the planning pass. It consumes:
//!   - `&[LoudnessSegment]` from Stage 1.5b
//!   - `&LoudnessStats` from Stage 1.5a (uses `signal_priority_map`)
//!
//! And produces:
//!   - `LoudnessPlan` — segments + pipeline latency constant
//!   - `Vec<LookaheadFrame>` — one frame per block, consumed by Stage 6.2
//!
//! # gain_db / gain_offset_db — Decibels(0.0) placeholder
//!
//! Per Q1 Option B (approved): both fields are `Decibels(0.0)` from this
//! stage. Stage 6.3 is the single authoritative normalization pass.
//! Using unweighted planning-pass LUFS here would risk double-normalization.
//!
//! # transient_flag — retained per v2.9 removal gate
//!
//! Per §Signal Priority Alignment: `transient_flag` is removed from
//! `LookaheadFrame` only if `debug_assert_eq!` fires zero times across all
//! dev cycles [11]–[18]. That gate has not been cleared. Field is retained.
//! The Synthesizer sets it from `signal_priority_map[block_index]`.
//!
//! # bypass_flag — always false from planning pass
//!
//! Stage 5.5 is the only entity that writes `true` to `bypass_flag` at
//! runtime (GR > 0.5dB). The Synthesizer cannot know this pre-execution.
//!
//! # Latency constant
//!
//! `offset_samples = Samples(640)` — compile-time, per §Latency Compensation:
//!   rubato_latency_samples(128) + fft_eq_latency_samples(512) = 640 @48kHz

use alloc::vec::Vec;

use crate::types::units::{Decibels, Samples};
use super::signal_priority::SignalPriority;
use super::stage1_5a_analyzer::LoudnessStats;
use super::stage1_5b_segmenter::LoudnessSegment;

// ── Compile-time constants ────────────────────────────────────────────────────

/// Pipeline latency per §Latency Compensation (compile-time, never runtime).
/// rubato (sinc quality=256): 128 samples
/// FFT EQ (fft_size=1024):    512 samples
/// Total:                     640 samples @ 48kHz (~13.3ms)
const PIPELINE_OFFSET_SAMPLES: Samples = Samples(640);

// ─────────────────────────────────────────────────────────────────────────────

/// The loudness plan produced by Stage 1.5c.
///
/// Per spec §Stage 1.5 (verbatim):
/// ```text
/// pub struct LoudnessPlan {
///     pub segments:       Vec<LoudnessSegment>,
///     pub offset_samples: Samples,  // typed (v2.9)
/// }
/// ```
///
/// Consumed by the Control Scheduler at §Control Tick Execution Order step 2c:
/// "LoudnessPlan segment gain (every block — lookup only)".
#[derive(Debug)]
pub struct LoudnessPlan {
    /// Contiguous loudness segments covering the full track (from Stage 1.5b).
    pub segments:       Vec<LoudnessSegment>,
    /// Pipeline latency: `Samples(640)` @ 48kHz.
    /// Consumed by Stage 6.2 for lookahead alignment.
    pub offset_samples: Samples,
}

/// Per-block lookahead frame consumed by Stage 6.2 (True Peak Limiter).
///
/// Per spec §Stage 1.5 (verbatim):
/// ```text
/// pub struct LookaheadFrame {
///     pub gain_db:        Decibels,  // typed (v2.9)
///     pub transient_flag: bool,      // retained — see §Signal Priority alignment
///     pub bypass_flag:    bool,      // true when Stage 5.5 active
///     pub offset_samples: Samples,   // typed (v2.9)
///     pub block_index:    u64,
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LookaheadFrame {
    /// Normalization gain — `Decibels(0.0)` from planning pass.
    /// Stage 6.3 is the authoritative normalization pass (Q1 Option B).
    pub gain_db:        Decibels,
    /// True when `signal_priority_map[block_index] == SignalPriority::Transient`.
    /// Retained in v2.9 pending §Signal Priority Alignment removal gate.
    pub transient_flag: bool,
    /// Set to `true` by Stage 5.5 at runtime when GR > 0.5dB.
    /// Always `false` from the Synthesizer (planning pass cannot know this).
    pub bypass_flag:    bool,
    /// Pipeline latency offset: `Samples(640)` @ 48kHz.
    pub offset_samples: Samples,
    /// Zero-based block index. Enables `debug_assert_eq!` alignment check
    /// in Stage 6.2 per §Stage 6 / signal_priority_map Alignment.
    pub block_index:    u64,
}

/// Synthesize a `LoudnessPlan` and `Vec<LookaheadFrame>` from the Stage 1.5
/// planning-pass outputs.
///
/// # Parameters
/// - `segments` — output of Stage 1.5b
/// - `stats` — output of Stage 1.5a; uses `signal_priority_map`
/// - `_intent_lufs` — reserved; `None` = raw mode (no normalization target).
///   Not used for gain computation — Stage 6.3 is the normalization authority.
pub fn synthesize(
    segments:     &[LoudnessSegment],
    stats:        &LoudnessStats,
    _intent_lufs: Option<f32>,
) -> (LoudnessPlan, Vec<LookaheadFrame>) {
    // ── LoudnessPlan ─────────────────────────────────────────────────────────
    let plan = LoudnessPlan {
        segments:       segments.to_vec(),
        offset_samples: PIPELINE_OFFSET_SAMPLES,
    };

    // ── LookaheadFrame queue — one per block ──────────────────────────────────
    let num_blocks = stats.signal_priority_map.len();
    let mut frames: Vec<LookaheadFrame> = Vec::with_capacity(num_blocks);

    for (block_index, &priority) in stats.signal_priority_map.iter().enumerate() {
        frames.push(LookaheadFrame {
            gain_db:        Decibels(0.0),                          // Q1 Option B
            transient_flag: priority == SignalPriority::Transient,  // retained v2.9
            bypass_flag:    false,                                  // Stage 5.5 sets this
            offset_samples: PIPELINE_OFFSET_SAMPLES,
            block_index:    block_index as u64,
        });
    }

    (plan, frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::stage1_5b_segmenter::LoudnessSegment;

    /// Build a minimal LoudnesStats with a given SignalPriority pattern.
    fn stats_with_priorities(priorities: &[SignalPriority]) -> LoudnessStats {
        let n = priorities.len();
        LoudnessStats {
            short_term_lufs_map:     (0..n).map(|i| (i as u64, -20.0f32)).collect(),
            median_crest_factor:     1.41,
            per_block_crest_factors: alloc::vec![1.41f32; n],
            signal_priority_map:     priorities.to_vec(),
        }
    }

    fn one_segment(lufs: f32) -> LoudnessSegment {
        LoudnessSegment {
            start_ms:       0,
            end_ms:         400,
            target_lufs:    lufs,
            gain_offset_db: Decibels(0.0),
        }
    }

    #[test]
    fn test_synthesizer_empty_stats() {
        // No blocks → empty frame queue; LoudnessPlan has correct offset.
        let stats = stats_with_priorities(&[]);
        let segs = alloc::vec![one_segment(-20.0)];
        let (plan, frames) = synthesize(&segs, &stats, Some(-14.0));

        assert_eq!(plan.offset_samples, Samples(640),
            "offset_samples must be compile-time Samples(640)");
        assert_eq!(plan.segments.len(), 1);
        assert!(frames.is_empty(), "no blocks → no frames");
    }

    #[test]
    fn test_synthesizer_offset_constant() {
        // Regardless of input, offset_samples must always be Samples(640).
        let stats = stats_with_priorities(&[SignalPriority::Body; 5]);
        let segs = alloc::vec![one_segment(-18.0)];
        let (plan, frames) = synthesize(&segs, &stats, None);

        assert_eq!(plan.offset_samples, Samples(640));
        assert_eq!(frames.len(), 5);
        for f in &frames {
            assert_eq!(f.offset_samples, Samples(640),
                "every frame must carry offset_samples = 640");
        }
    }

    #[test]
    fn test_synthesizer_gain_placeholder() {
        // All gain_db values must be Decibels(0.0) — planning placeholder.
        let stats = stats_with_priorities(&[SignalPriority::Body; 3]);
        let segs = alloc::vec![one_segment(-20.0)];
        let (_, frames) = synthesize(&segs, &stats, Some(-14.0));

        for (i, f) in frames.iter().enumerate() {
            assert_eq!(f.gain_db.0, 0.0,
                "frame[{i}]: gain_db must be 0.0 (Q1 Option B placeholder)");
            assert!(!f.bypass_flag,
                "frame[{i}]: bypass_flag must be false from Synthesizer");
        }
    }

    #[test]
    fn test_synthesizer_transient_flag() {
        // Frames whose block has SignalPriority::Transient get transient_flag=true.
        let priorities = alloc::vec![
            SignalPriority::Body,
            SignalPriority::Transient,
            SignalPriority::Body,
            SignalPriority::Noise,
            SignalPriority::Transient,
        ];
        let stats = stats_with_priorities(&priorities);
        let segs = alloc::vec![one_segment(-20.0)];
        let (_, frames) = synthesize(&segs, &stats, None);

        assert_eq!(frames.len(), 5);
        assert!(!frames[0].transient_flag, "Body → transient_flag = false");
        assert!( frames[1].transient_flag, "Transient → transient_flag = true");
        assert!(!frames[2].transient_flag, "Body → transient_flag = false");
        assert!(!frames[3].transient_flag, "Noise → transient_flag = false");
        assert!( frames[4].transient_flag, "Transient → transient_flag = true");
    }

    #[test]
    fn test_synthesizer_block_index_sequential() {
        // block_index must be sequential 0..n.
        let n = 8usize;
        let stats = stats_with_priorities(&alloc::vec![SignalPriority::Body; n]);
        let segs = alloc::vec![one_segment(-20.0)];
        let (_, frames) = synthesize(&segs, &stats, None);

        assert_eq!(frames.len(), n);
        for (i, f) in frames.iter().enumerate() {
            assert_eq!(f.block_index, i as u64,
                "frame[{i}]: block_index must be {i}, got {}", f.block_index);
        }
    }
}
