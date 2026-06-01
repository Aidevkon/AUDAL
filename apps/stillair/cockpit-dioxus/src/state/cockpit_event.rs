//! CockpitEvent — all legal UI state transitions.
//! Authority: ARCHITECTURE v3.4 §Cockpit Reducer
//! Every mode.set() call in the codebase maps to one event here.

#[derive(Debug, Clone, PartialEq)]
pub enum CockpitEvent {
    // File lifecycle
    FileDropped { path: String, name: String, format: String },
    FileDropFailed { message: String },
    BackToIdle,

    // Preset selection
    PresetSelected { preset_id: String },

    // Mastering
    MasterTriggered,
    MasteringFailed { message: String },
    MasteringComplete { blob_id: String },

    // Export
    ExportTriggered { format: String },
    ExportComplete,
    ExportFailed { message: String },

    // System
    FaultAcknowledged,

    // JINI (J-P6)
    JiniSuggestionReady     { suggestion: crate::types::JiniSuggestionJson },
    JiniSuggestionAccepted,
    JiniSuggestionDismissed,
    JiniPersonaChanged      { to: crate::types::JiniPersonaState },
}
