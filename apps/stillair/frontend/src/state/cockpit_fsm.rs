//! CockpitFsm — Double-Lock typestate FSM.
//! Authority: state-machine.md §6.1 · Phase 5 task-decomposition P5-002
//!
//! Layer 1 (compile-time): InternalCockpit<S> typestate — prevents
//!   calling start_mastering() without prior preset_selected().
//!   Compiler enforces the transition table from state-machine.md §4.
//!
//! Layer 2 (runtime): CockpitMode enum — read by MFD panels for rendering.
//!   Always in sync with S. Never diverges.
//!
//! FORBIDDEN: Rendering CockpitMode directly from InternalCockpit<S> generics.
//! FORBIDDEN: FM-ERR → FM1 shortcut (must go through FM0).

use std::marker::PhantomData;
use super::cockpit_mode::{AscCode, CockpitMode};

// ── Typestate markers ─────────────────────────────────────────────────────────
#[derive(Debug)] pub struct Idle;
#[derive(Debug)] pub struct FileLoaded;
#[derive(Debug)] pub struct PresetSelected;
#[derive(Debug)] pub struct Mastering;
#[derive(Debug)] pub struct Mastered;
#[derive(Debug)] pub struct InsightsReady;
#[derive(Debug)] pub struct CoachReady;
#[derive(Debug)] pub struct Exporting;

/// Sealed Intent — created at FM1.5 → FM2 transition.
/// Once created, Intent is immutable. Passed directly to sp314-dsp (Phase 6).
/// authority: state-machine.md §6.2
#[derive(Debug, Clone)]
pub struct Intent {
    pub audio_path: String,
    pub preset_id:  &'static str,
}

/// The internal FSM. State S is compile-time — transitions are type-safe.
/// `mode` field is always in sync with S for MFD rendering.
#[derive(Debug)]
pub struct InternalCockpit<S> {
    _state: PhantomData<S>,
    /// Current flight mode — read by Cockpit component for MFD rendering.
    pub mode: CockpitMode,
    /// Audio file path (set at FM0 → FM1).
    pub audio_path: Option<String>,
    /// Selected preset (set at FM1 → FM1.5).
    pub preset_id: Option<&'static str>,
}

// ── FM0: Idle ─────────────────────────────────────────────────────────────────
impl InternalCockpit<Idle> {
    pub fn new() -> Self {
        Self {
            _state: PhantomData,
            mode: CockpitMode::Idle,
            audio_path: None,
            preset_id: None,
        }
    }

    /// FM0 → FM1: file_dropped
    /// Guard: file passes sanitization (Phase 6 adds real validation).
    /// Returns Err(ValidationFail) if file is invalid.
    pub fn file_dropped(self, path: String)
        -> Result<InternalCockpit<FileLoaded>, (InternalCockpit<Idle>, AscCode)>
    {
        // Phase 6: real sanitization (check dBFS ceiling, sample rate, etc.)
        // Phase 5: stub — all files pass
        if path.is_empty() {
            return Err((
                InternalCockpit { _state: PhantomData, mode: CockpitMode::Idle,
                    audio_path: None, preset_id: None },
                AscCode::ValidationFail,
            ));
        }
        Ok(InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::FileLoaded,
            audio_path: Some(path),
            preset_id: None,
        })
    }
}

// ── FM1: FileLoaded ───────────────────────────────────────────────────────────
impl InternalCockpit<FileLoaded> {
    /// FM1 → FM1.5: preset_selected
    /// Guard: preset must exist in bmr-128.schema.json (Phase 6 validates against schema).
    pub fn preset_selected(self, preset_id: &'static str) -> InternalCockpit<PresetSelected> {
        InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::PresetSelected,
            audio_path: self.audio_path,
            preset_id: Some(preset_id),
        }
    }
}

// ── FM1.5: PresetSelected ─────────────────────────────────────────────────────
impl InternalCockpit<PresetSelected> {
    /// FM1.5 → FM1: abort
    pub fn abort(self) -> InternalCockpit<FileLoaded> {
        InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::FileLoaded,
            audio_path: self.audio_path,
            preset_id: None,
        }
    }

    /// FM1.5 → FM2: start_mastering (seals Intent)
    /// Compiler prevents calling this without prior preset_selected().
    /// Intent is immutable after creation — state-machine.md §6.2.
    pub fn start_mastering(self) -> (InternalCockpit<Mastering>, Intent) {
        let audio_path = self.audio_path.clone().unwrap_or_default();
        let preset_id  = self.preset_id.unwrap_or("spotify");
        let intent = Intent { audio_path: audio_path.clone(), preset_id };
        let fsm = InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::Mastering,
            audio_path: Some(audio_path),
            preset_id: Some(preset_id),
        };
        (fsm, intent)
    }
}

// ── FM2: Mastering ────────────────────────────────────────────────────────────
impl InternalCockpit<Mastering> {
    /// FM2 → FM3: dsp_done (Golden Blob produced)
    pub fn dsp_done(self) -> InternalCockpit<Mastered> {
        InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::Mastered,
            audio_path: self.audio_path,
            preset_id: self.preset_id,
        }
    }

    /// FM2 → FM1: abort (user-initiated or Aborted ASC)
    pub fn abort(self) -> InternalCockpit<FileLoaded> {
        InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::FileLoaded,
            audio_path: self.audio_path,
            preset_id: None,
        }
    }
}

// ── FM3: Mastered ─────────────────────────────────────────────────────────────
impl InternalCockpit<Mastered> {
    /// FM3 → FM4: analysis_done (automatic — Data Cascade)
    pub fn analysis_done(self) -> InternalCockpit<InsightsReady> {
        InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::InsightsReady,
            audio_path: self.audio_path,
            preset_id: self.preset_id,
        }
    }

    /// FM3 → FM0: load_new_file (atomic reset — hard_reset() internally)
    pub fn load_new_file(self) -> InternalCockpit<Idle> {
        InternalCockpit::new()
    }
}

// ── FM4: InsightsReady ────────────────────────────────────────────────────────
impl InternalCockpit<InsightsReady> {
    /// FM4 → FM5: coach_done (automatic — Data Cascade)
    pub fn coach_done(self) -> InternalCockpit<CoachReady> {
        InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::CoachReady,
            audio_path: self.audio_path,
            preset_id: self.preset_id,
        }
    }

    /// FM4 → FM0: load_new_file
    pub fn load_new_file(self) -> InternalCockpit<Idle> {
        InternalCockpit::new()
    }
}

// ── FM5: CoachReady ───────────────────────────────────────────────────────────
impl InternalCockpit<CoachReady> {
    /// FM5 → FM6: export_clicked
    pub fn export_clicked(self) -> InternalCockpit<Exporting> {
        InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::Exporting,
            audio_path: self.audio_path,
            preset_id: self.preset_id,
        }
    }

    /// FM5 → FM0: load_new_file
    pub fn load_new_file(self) -> InternalCockpit<Idle> {
        InternalCockpit::new()
    }
}

// ── FM6: Exporting ────────────────────────────────────────────────────────────
impl InternalCockpit<Exporting> {
    /// FM6 → FM5: export_done (UI unlocked, Golden Blob retained)
    pub fn export_done(self) -> InternalCockpit<CoachReady> {
        InternalCockpit {
            _state: PhantomData,
            mode: CockpitMode::CoachReady,
            audio_path: self.audio_path,
            preset_id: self.preset_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idle_to_file_loaded() {
        let fsm = InternalCockpit::<Idle>::new();
        assert_eq!(fsm.mode, CockpitMode::Idle);
        let fsm = fsm.file_dropped("/tmp/track.wav".into()).unwrap();
        assert_eq!(fsm.mode, CockpitMode::FileLoaded);
    }

    #[test]
    fn test_empty_path_fails_validation() {
        let fsm = InternalCockpit::<Idle>::new();
        let err = fsm.file_dropped("".into());
        assert!(err.is_err());
        let (_, code) = err.unwrap_err();
        assert_eq!(code, AscCode::ValidationFail);
    }

    #[test]
    fn test_full_happy_path_fm0_to_fm5() {
        let fsm = InternalCockpit::<Idle>::new();
        let fsm = fsm.file_dropped("/tmp/track.wav".into()).unwrap();
        let fsm = fsm.preset_selected("spotify");
        assert_eq!(fsm.mode, CockpitMode::PresetSelected);
        let (fsm, intent) = fsm.start_mastering();
        assert_eq!(fsm.mode, CockpitMode::Mastering);
        assert_eq!(intent.preset_id, "spotify");
        let fsm = fsm.dsp_done();
        assert_eq!(fsm.mode, CockpitMode::Mastered);
        let fsm = fsm.analysis_done();
        assert_eq!(fsm.mode, CockpitMode::InsightsReady);
        let fsm = fsm.coach_done();
        assert_eq!(fsm.mode, CockpitMode::CoachReady);
    }

    #[test]
    fn test_preset_selected_abort_returns_to_file_loaded() {
        let fsm = InternalCockpit::<Idle>::new()
            .file_dropped("/tmp/t.wav".into()).unwrap()
            .preset_selected("apple_music");
        let fsm = fsm.abort();
        assert_eq!(fsm.mode, CockpitMode::FileLoaded);
    }

    #[test]
    fn test_mastering_abort_returns_to_file_loaded() {
        let fsm = InternalCockpit::<Idle>::new()
            .file_dropped("/tmp/t.wav".into()).unwrap()
            .preset_selected("spotify");
        let (fsm, _intent) = fsm.start_mastering();
        let fsm = fsm.abort();
        assert_eq!(fsm.mode, CockpitMode::FileLoaded);
    }

    #[test]
    fn test_load_new_file_resets_to_idle() {
        let fsm = InternalCockpit::<Idle>::new()
            .file_dropped("/tmp/t.wav".into()).unwrap()
            .preset_selected("spotify");
        let (fsm, _) = fsm.start_mastering();
        let fsm = fsm.dsp_done().analysis_done().coach_done();
        assert_eq!(fsm.mode, CockpitMode::CoachReady);
        let fsm = fsm.load_new_file();
        assert_eq!(fsm.mode, CockpitMode::Idle);
    }

    #[test]
    fn test_export_flow() {
        let fsm = InternalCockpit::<Idle>::new()
            .file_dropped("/tmp/t.wav".into()).unwrap()
            .preset_selected("spotify");
        let (fsm, _) = fsm.start_mastering();
        let fsm = fsm.dsp_done().analysis_done().coach_done();
        let fsm = fsm.export_clicked();
        assert_eq!(fsm.mode, CockpitMode::Exporting);
        let fsm = fsm.export_done();
        assert_eq!(fsm.mode, CockpitMode::CoachReady);
    }

    #[test]
    fn test_intent_carries_correct_preset() {
        let fsm = InternalCockpit::<Idle>::new()
            .file_dropped("/tmp/t.wav".into()).unwrap()
            .preset_selected("broadcast");
        let (_, intent) = fsm.start_mastering();
        assert_eq!(intent.preset_id, "broadcast");
        assert_eq!(intent.audio_path, "/tmp/t.wav");
    }
}
