//! Onboarding state contracts.
//! Authority: hangar-onboarding-spec v1.0 · onboarding.schema.json v1.0
//! v1.0: in-memory only. v2: persisted via M0 daemon.

use serde::{Deserialize, Serialize};
use crate::jini::JiniPersonaId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum WizardState {
    #[default]
    Initial,
    VisionSelected,
    TasteSet,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum Vision {
    #[default]
    Neutral,
    Music,
    Podcast,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TasteProfile {
    pub warmth:     f32,   // 0.0-1.0, default 0.5
    pub clarity:    f32,
    pub punch:      f32,
    pub brightness: f32,
}

impl Default for TasteProfile {
    fn default() -> Self {
        Self { warmth: 0.5, clarity: 0.5, punch: 0.5, brightness: 0.5 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum PlatformTarget {
    Spotify,
    Youtube,
    Broadcast,
    #[default]
    Undecided,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingState {
    pub wizard_state:       WizardState,
    pub sessions_completed: u32,
    pub persona:            JiniPersonaId,
    pub persona_override:   Option<JiniPersonaId>,
    pub vision:             Vision,
    pub taste:              TasteProfile,
    pub platform_target:    PlatformTarget,
    pub completed_at:       Option<u64>,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            wizard_state:       WizardState::Initial,
            sessions_completed: 0,
            persona:            JiniPersonaId::Beginner,
            persona_override:   None,
            vision:             Vision::default(),
            taste:              TasteProfile::default(),
            platform_target:    PlatformTarget::default(),
            completed_at:       None,
        }
    }
}

impl OnboardingState {
    /// Derive persona from sessions_completed (unless override is set)
    pub fn resolve_persona(&mut self) {
        if self.persona_override.is_none() {
            self.persona = match self.sessions_completed {
                0..=4   => JiniPersonaId::Beginner,
                5..=19  => JiniPersonaId::Intermediate,
                _       => JiniPersonaId::Pro,
            };
        }
    }

    pub fn is_completed(&self) -> bool {
        self.wizard_state == WizardState::Completed
    }
}
