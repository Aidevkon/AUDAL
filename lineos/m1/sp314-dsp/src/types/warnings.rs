//! Pipeline warning types — sp314-dsp v2.9 §Warning Aggregator.
//! Data types live in `types` so `GoldenBlob` can reference them without a pipeline cycle.

/// Source stage for `PipelineWarning::LimiterOverwork`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimiterOverworkSource {
    PreLimiterSafety,
    LimiterStage,
}

/// Non-fatal pipeline warnings (`MemoryPressure` removed — impossible with `StagePool`).
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineWarning {
    GainBudgetExceeded,
    GainBudgetStarved,
    PhaseDriftDetected,
    LimiterOverwork { source: LimiterOverworkSource },
    MonoCollapseRisk,
    LufsTargetMiss,
    LookaheadStarvation,
}

impl PipelineWarning {
    /// Match by variant shape only (`LimiterOverwork` sources are distinct).
    pub fn same_discriminant(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::GainBudgetExceeded, Self::GainBudgetExceeded) => true,
            (Self::GainBudgetStarved, Self::GainBudgetStarved) => true,
            (Self::PhaseDriftDetected, Self::PhaseDriftDetected) => true,
            (Self::LimiterOverwork { source: a }, Self::LimiterOverwork { source: b }) => a == b,
            (Self::MonoCollapseRisk, Self::MonoCollapseRisk) => true,
            (Self::LufsTargetMiss, Self::LufsTargetMiss) => true,
            (Self::LookaheadStarvation, Self::LookaheadStarvation) => true,
            _ => false,
        }
    }
}

/// Aggregated warning record (first occurrence + counts).
#[derive(Debug, Clone, PartialEq)]
pub struct WarningRecord {
    pub warning:     PipelineWarning,
    pub count:       u32,
    pub first_block: u64,
    pub last_block:  u64,
}
