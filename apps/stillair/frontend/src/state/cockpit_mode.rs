//! CockpitMode — view-only enum for MFD rendering.
//! Authority: state-machine.md §6.1 · Phase 5 task-decomposition P5-002
//!
//! FORBIDDEN: Never set this directly from UI components.
//! Only InternalCockpit<S> transitions produce a new CockpitMode.
//! This enum is read-only from the rendering layer's perspective.

use serde::{Deserialize, Serialize};

/// View-only flight modes for MFD rendering.
/// Corresponds 1:1 with the FSM states in state-machine.md §2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CockpitMode {
    /// FM0: Cold & Dark. Awaiting file load.
    Idle,
    /// FM1: File decoded, metadata extracted. Preset menu available.
    FileLoaded,
    /// FM1.5: Intent sealed. Ready for mastering.
    PresetSelected,
    /// FM2: sp314-dsp executing. DSP pipeline running.
    Mastering,
    /// FM3: Golden Blob produced. DSP complete.
    Mastered,
    /// FM4: Telemetry + EBU R128 measurements available.
    InsightsReady,
    /// FM5: Rule-engine findings available. Steady cruise.
    CoachReady,
    /// FM6: I/O in progress. UI locked. Golden Blob writing.
    Exporting,
    /// FM-ERR: Determinism chain broken. Locked to safe state.
    Fault(AscCode),
}

/// Avionics Status Codes — emitted on FM-ERR transition.
/// Per state-machine.md §3. Never shown raw to user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AscCode {
    /// 0x01 — NaN or Inf detected in DSP output
    MathErr        = 0x01,
    /// 0x02 — File read / Golden Blob write failure
    IoErr          = 0x02,
    /// 0x03 — User-initiated emergency stop during FM2
    Aborted        = 0x03,
    /// 0x04 — Input audio violates sanitization rules
    ValidationFail = 0x04,
    /// 0x05 — LineOS runtime crash
    WasmPanic      = 0x05,
}

impl CockpitMode {
    /// UI interactions are allowed in all modes except Mastering, Exporting, Fault.
    pub fn is_interactive(&self) -> bool {
        !matches!(self, Self::Mastering | Self::Exporting | Self::Fault(_))
    }

    /// Coach panel renders findings only in CoachReady.
    pub fn shows_coach(&self) -> bool {
        matches!(self, Self::CoachReady)
    }

    /// Mode label for transport bar display.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle           => "FM0  IDLE",
            Self::FileLoaded     => "FM1  FILE LOADED",
            Self::PresetSelected => "FM1.5  INTENT SEALED",
            Self::Mastering      => "FM2  MASTERING",
            Self::Mastered       => "FM3  MASTERED",
            Self::InsightsReady  => "FM4  INSIGHTS READY",
            Self::CoachReady     => "FM5  COACH READY",
            Self::Exporting      => "FM6  EXPORTING",
            Self::Fault(_)       => "FM-ERR  FAULT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_interactive_allows_idle() {
        assert!(CockpitMode::Idle.is_interactive());
    }

    #[test]
    fn test_is_interactive_blocks_mastering() {
        assert!(!CockpitMode::Mastering.is_interactive());
    }

    #[test]
    fn test_is_interactive_blocks_exporting() {
        assert!(!CockpitMode::Exporting.is_interactive());
    }

    #[test]
    fn test_is_interactive_blocks_fault() {
        assert!(!CockpitMode::Fault(AscCode::MathErr).is_interactive());
    }

    #[test]
    fn test_shows_coach_only_in_coach_ready() {
        assert!( CockpitMode::CoachReady.shows_coach());
        assert!(!CockpitMode::InsightsReady.shows_coach());
        assert!(!CockpitMode::Idle.shows_coach());
        assert!(!CockpitMode::Mastered.shows_coach());
    }

    #[test]
    fn test_labels_are_non_empty() {
        let all_modes = [
            CockpitMode::Idle,
            CockpitMode::FileLoaded,
            CockpitMode::PresetSelected,
            CockpitMode::Mastering,
            CockpitMode::Mastered,
            CockpitMode::InsightsReady,
            CockpitMode::CoachReady,
            CockpitMode::Exporting,
            CockpitMode::Fault(AscCode::WasmPanic),
        ];
        for mode in &all_modes {
            assert!(!mode.label().is_empty(), "Label missing for {mode:?}");
        }
    }
}
