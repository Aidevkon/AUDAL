//! cockpit_mode.rs — CockpitMode FSM for Dioxus Cockpit.
//! Authority: state-machine.md · Phase 11 P11-002
//!
//! Same states as the Leptos FSM, translated to a plain Rust enum.
//! CockpitMode carries data directly — no separate signals for path/preset.
//! This is the key architectural improvement over the Leptos implementation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AscCode {
    MathErr        = 0x01,
    IoErr          = 0x02,
    Aborted        = 0x03,
    ValidationFail = 0x04,
    WasmPanic      = 0x05,
    FileReadError  = 0x10,
    MasteringError = 0x11,
    ExportError    = 0x12,
}

impl AscCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MathErr        => "ASC 0x01",
            Self::IoErr          => "ASC 0x02",
            Self::Aborted        => "ASC 0x03",
            Self::ValidationFail => "ASC 0x04",
            Self::WasmPanic      => "ASC 0x05",
            Self::FileReadError  => "ASC 0x10",
            Self::MasteringError => "ASC 0x11",
            Self::ExportError    => "ASC 0x12",
        }
    }
}

/// Dioxus Cockpit FSM — mirrors state-machine.md §2 exactly.
///
/// Key difference from Leptos: CockpitMode carries data (path, preset_id,
/// blob_id) inside the variant. No separate signals needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CockpitMode {
    /// FM0 — Cold & Dark. Awaiting file load.
    Idle,

    /// FM1 — File decoded, metadata extracted. Preset menu available.
    FileLoaded {
        path:   String,
        name:   String,
        format: String,
    },

    /// FM1.5 — Intent sealed. Ready for mastering.
    PresetSelected {
        path:      String,
        name:      String,
        preset_id: String,
    },

    /// FM2 — sp314-dsp executing. DSP pipeline running.
    Mastering {
        path:      String,
        preset_id: String,
    },

    /// FM5 — Rule-engine findings available. Steady cruise.
    /// Note: FM3/FM4 are collapsed — getSessionState() handles the cascade.
    CoachReady {
        blob_id: String,
    },

    /// FM6 — I/O in progress. UI locked.
    Exporting {
        blob_id: String,
        format:  String,
    },

    /// FM-ERR — Unrecoverable error. MASTER RESET → FM0.
    Fault {
        code:    AscCode,
        message: String,
    },
}

impl CockpitMode {
    /// UI interactions allowed in all modes except Mastering, Exporting, Fault.
    pub fn is_interactive(&self) -> bool {
        !matches!(self,
            Self::Mastering { .. } | Self::Exporting { .. } | Self::Fault { .. }
        )
    }

    /// Transport bar label — avionics mode indicator.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle              => "FM0  IDLE",
            Self::FileLoaded { .. } => "FM1  FILE LOADED",
            Self::PresetSelected{..}=> "FM1.5  PRESET SELECTED",
            Self::Mastering { .. }  => "FM2  MASTERING",
            Self::CoachReady { .. } => "FM5  COACH READY",
            Self::Exporting { .. }  => "FM6  EXPORTING",
            Self::Fault { .. }      => "FM-ERR",
        }
    }

    /// Extract blob_id if in CoachReady or Exporting state.
    pub fn blob_id(&self) -> Option<&str> {
        match self {
            Self::CoachReady { blob_id } | Self::Exporting { blob_id, .. }
                => Some(blob_id.as_str()),
            _   => None,
        }
    }

    /// True if currently mastering — disables transport controls.
    pub fn is_mastering(&self) -> bool {
        matches!(self, Self::Mastering { .. })
    }

    /// True if a file is loaded (any state after FM0 except Fault).
    pub fn has_file(&self) -> bool {
        !matches!(self, Self::Idle | Self::Fault { .. })
    }
}
