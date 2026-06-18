//! Cockpit Reducer — pure function UI state machine.
//! Authority: ARCHITECTURE v3.4 §Cockpit Reducer
//! reduce(CockpitMode, CockpitEvent) → CockpitMode
//! No I/O. No side effects. No Operator calls. Pure function only.

use super::cockpit_event::CockpitEvent;
use super::cockpit_mode::{AscCode, CockpitMode};
use dioxus::prelude::*;

/// Dispatch a CockpitEvent through the reducer into the mode Signal.
/// This is the ONLY place mode mutation happens — components never
/// call mode.set() directly.
pub fn dispatch(mut mode: Signal<CockpitMode>, event: CockpitEvent) {
    web_sys::console::error_1(&format!("[TRAP] DISPATCH: event={:?} mode={:?}", event, *mode.read()).into());
    let current = mode.read().clone();
    let next = reduce(current, event);
    web_sys::console::error_1(&format!("[TRAP] DISPATCH RESULT: next_mode={:?}", next).into());
    mode.set(next);
}

pub fn reduce(mode: CockpitMode, event: CockpitEvent) -> CockpitMode {
    match (mode, event) {
        // From Idle
        (CockpitMode::Idle, CockpitEvent::FileDropped { path, name, format }) => {
            CockpitMode::FileLoaded { path, name, format }
        }
        (CockpitMode::Idle, CockpitEvent::FileDropFailed { message }) => CockpitMode::Fault {
            code: AscCode::FileReadError,
            message,
        },

        // From FileLoaded
        (CockpitMode::FileLoaded { .. }, CockpitEvent::BackToIdle) => CockpitMode::Idle,
        (
            CockpitMode::FileLoaded { path, name, .. },
            CockpitEvent::PresetSelected { preset_id },
        ) => CockpitMode::PresetSelected {
            path,
            name,
            preset_id,
        },

        // From PresetSelected
        (CockpitMode::PresetSelected { .. }, CockpitEvent::BackToIdle) => CockpitMode::Idle,
        (
            CockpitMode::PresetSelected {
                path, preset_id, ..
            },
            CockpitEvent::MasterTriggered,
        ) => CockpitMode::Mastering { path, preset_id },

        // From Mastering
        (CockpitMode::Mastering { .. }, CockpitEvent::MasteringFailed { message }) => {
            CockpitMode::Fault {
                code: AscCode::MasteringError,
                message,
            }
        }
        (CockpitMode::Mastering { .. }, CockpitEvent::MasteringComplete { blob_id }) => {
            CockpitMode::CoachReady { blob_id }
        }

        // From CoachReady
        (CockpitMode::CoachReady { blob_id }, CockpitEvent::ExportTriggered { format }) => {
            CockpitMode::Exporting { blob_id, format }
        }

        // From Exporting
        (CockpitMode::Exporting { blob_id, .. }, CockpitEvent::ExportComplete) => {
            CockpitMode::CoachReady { blob_id }
        }
        (CockpitMode::Exporting { .. }, CockpitEvent::ExportFailed { message }) => {
            CockpitMode::Fault {
                code: AscCode::ExportError,
                message,
            }
        }

        // From Fault
        (CockpitMode::Fault { .. }, CockpitEvent::FaultAcknowledged) => CockpitMode::Idle,

        // ── JINI transitions (J-P6) ─────────────────────────────────────────
        // Suggestion ready — mode stays CoachReady, suggestion stored via signal
        (CockpitMode::CoachReady { blob_id }, CockpitEvent::JiniSuggestionReady { .. }) => {
            CockpitMode::CoachReady { blob_id }
        }
        // Accept/Dismiss/Persona — side effect only, mode unchanged
        (mode, CockpitEvent::JiniSuggestionAccepted) => mode,
        (mode, CockpitEvent::JiniSuggestionDismissed) => mode,
        (mode, CockpitEvent::JiniPersonaChanged { .. }) => mode,

        // Abort wildcard — allow reset from any state
        (_, CockpitEvent::BackToIdle) => CockpitMode::Idle,

        // Illegal transitions — return mode unchanged, no panic
        (mode, _) => mode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_file_dropped_transitions_to_file_loaded() {
        let result = reduce(
            CockpitMode::Idle,
            CockpitEvent::FileDropped {
                path: "/tmp/test.wav".into(),
                name: "test.wav".into(),
                format: "WAV".into(),
            },
        );
        assert!(matches!(result, CockpitMode::FileLoaded { .. }));
    }

    #[test]
    fn file_loaded_preset_selected_transitions() {
        let result = reduce(
            CockpitMode::FileLoaded {
                path: "/tmp/test.wav".into(),
                name: "test.wav".into(),
                format: "WAV".into(),
            },
            CockpitEvent::PresetSelected {
                preset_id: "spotify_v3".into(),
            },
        );
        match result {
            CockpitMode::PresetSelected { preset_id, .. } => assert_eq!(preset_id, "spotify_v3"),
            other => panic!("Expected PresetSelected, got {:?}", other),
        }
    }

    #[test]
    fn mastering_complete_transitions_to_coach_ready() {
        let result = reduce(
            CockpitMode::Mastering {
                path: "/tmp/test.wav".into(),
                preset_id: "spotify_v3".into(),
            },
            CockpitEvent::MasteringComplete {
                blob_id: "blob_123".into(),
            },
        );
        assert!(matches!(result, CockpitMode::CoachReady { .. }));
    }

    #[test]
    fn fault_acknowledged_returns_to_idle() {
        let result = reduce(
            CockpitMode::Fault {
                code: AscCode::FileReadError,
                message: "test".into(),
            },
            CockpitEvent::FaultAcknowledged,
        );
        assert_eq!(result, CockpitMode::Idle);
    }

    #[test]
    fn illegal_transition_returns_mode_unchanged() {
        let mode = CockpitMode::Idle;
        let result = reduce(mode.clone(), CockpitEvent::MasterTriggered);
        assert_eq!(result, CockpitMode::Idle);
    }

    #[test]
    fn full_happy_path_fsm() {
        let mut mode = CockpitMode::Idle;

        mode = reduce(
            mode,
            CockpitEvent::FileDropped {
                path: "/music/song.wav".into(),
                name: "song.wav".into(),
                format: "WAV".into(),
            },
        );
        assert!(matches!(mode, CockpitMode::FileLoaded { .. }));

        mode = reduce(
            mode,
            CockpitEvent::PresetSelected {
                preset_id: "podcast_voice".into(),
            },
        );
        assert!(matches!(mode, CockpitMode::PresetSelected { .. }));

        mode = reduce(mode, CockpitEvent::MasterTriggered);
        assert!(matches!(mode, CockpitMode::Mastering { .. }));

        mode = reduce(
            mode,
            CockpitEvent::MasteringComplete {
                blob_id: "golden_001".into(),
            },
        );
        assert!(matches!(mode, CockpitMode::CoachReady { .. }));

        mode = reduce(
            mode,
            CockpitEvent::ExportTriggered {
                format: "WAV".into(),
            },
        );
        assert!(matches!(mode, CockpitMode::Exporting { .. }));

        mode = reduce(mode, CockpitEvent::ExportComplete);
        assert!(matches!(mode, CockpitMode::CoachReady { .. }));
    }
}
