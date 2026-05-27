// aether/control/types.rs — Control surface types
// Authority: spec/locked/S-011a_multimodal_control.md v1.0
// Pure logic — depends only on aether::personas + libm

use crate::personas::config::{PersonaConfig, MacroControls};

/// Tier 1 — Black Box mode presets
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BlackBoxMode {
    Clean, Warm, Punch, Air, Film, Broadcast,
}

/// Tier 1 — Black Box control
pub struct BlackBoxControl {
    pub mode: BlackBoxMode,
}

impl BlackBoxControl {
    /// Maps mode to (persona_id, MacroControls).
    /// Compile-time constant mapping — deterministic.
    pub fn to_intent(&self) -> (&'static str, MacroControls) {
        match self.mode {
            BlackBoxMode::Clean =>
                ("hybrid_hifi", MacroControls {
                    warmth:0.3, punch:0.5,
                    forwardness:0.5, smoothness:0.7 }),
            BlackBoxMode::Warm =>
                ("warm_analog", MacroControls {
                    warmth:0.8, punch:0.4,
                    forwardness:0.4, smoothness:0.6 }),
            BlackBoxMode::Punch =>
                ("clean_punch", MacroControls {
                    warmth:0.3, punch:0.9,
                    forwardness:0.7, smoothness:0.3 }),
            BlackBoxMode::Air =>
                ("hybrid_hifi", MacroControls {
                    warmth:0.4, punch:0.4,
                    forwardness:0.8, smoothness:0.6 }),
            BlackBoxMode::Film =>
                ("cinematic_wide", MacroControls {
                    warmth:0.7, punch:0.5,
                    forwardness:0.4, smoothness:0.7 }),
            BlackBoxMode::Broadcast =>
                ("hybrid_hifi", MacroControls {
                    warmth:0.5, punch:0.5,
                    forwardness:0.5, smoothness:0.6 }),
        }
    }
}

/// 2D spatial orb position.
/// x: stereo width hint [-1.0, +1.0]
/// y: perceived depth/forwardness [-1.0, +1.0]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OrbPosition {
    pub x: f32,
    pub y: f32,
}

impl OrbPosition {
    pub fn center() -> Self { Self { x: 0.0, y: 0.0 } }

    /// Map orb to macro deltas.
    /// NOTE: orb mapping is INDEPENDENT of persona macro curves.
    /// It is a simple linear delta on top of current MacroControls.
    /// x → smoothness_delta (right = wider/smoother, UI concept)
    /// y → forwardness_delta (up = more forward/dry)
    pub fn to_macro_delta(&self) -> MacroDelta {
        MacroDelta {
            forwardness_delta: -self.y * 0.3,
            smoothness_delta:   self.x * 0.2,
        }
    }
}

/// Small delta applied on top of MacroControls from orb.
/// Applied by adding to current values then clamping to
/// persona's handle bounds before passing to S-004.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroDelta {
    pub forwardness_delta: f32,  // [-0.3, +0.3]
    pub smoothness_delta:  f32,  // [-0.2, +0.2]
}

impl MacroDelta {
    /// Apply delta to MacroControls.
    /// Clamped to persona's macro handle bounds (per S-004 §6).
    pub fn apply(&self, macros: &MacroControls,
                  persona: &PersonaConfig) -> MacroControls {
        MacroControls {
            warmth:      macros.warmth,
            punch:       macros.punch,
            forwardness: (macros.forwardness + self.forwardness_delta)
                .clamp(persona.macros.forwardness.min,
                       persona.macros.forwardness.max),
            smoothness:  (macros.smoothness + self.smoothness_delta)
                .clamp(persona.macros.smoothness.min,
                       persona.macros.smoothness.max),
        }
    }
}

/// Tier 2 — Medium control
#[derive(Debug, Clone)]
pub struct MediumControl {
    pub persona_id: String,
    pub macros:     MacroControls,
    pub orb:        OrbPosition,
    pub gain_match: bool,
}

impl MediumControl {
    pub fn new(persona_id: &str) -> Self {
        Self {
            persona_id: persona_id.into(),
            macros:     MacroControls::default(),
            orb:        OrbPosition::center(),
            gain_match: true,
        }
    }
}

/// Compute orb resistance coefficient [0.0, 1.0] for haptic feedback.
/// 0.0 = free movement, 1.0 = wall (maximum resistance).
pub fn orb_resistance(pos: &OrbPosition) -> f32 {
    let r = libm::sqrtf(pos.x * pos.x + pos.y * pos.y);
    if r < 0.3 {
        0.0
    } else if r < 0.7 {
        (r - 0.3) / 0.4
    } else {
        1.0_f32.min((r - 0.7) / 0.3 + 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personas::manager::PersonaManager;

    #[test]
    fn blackbox_mode_deterministic() {
        let (id1, m1) = BlackBoxControl{mode:BlackBoxMode::Warm}
            .to_intent();
        let (id2, m2) = BlackBoxControl{mode:BlackBoxMode::Warm}
            .to_intent();
        assert_eq!(id1, id2);
        assert_eq!(m1.warmth, m2.warmth);
    }

    #[test]
    fn blackbox_all_modes_valid_persona() {
        let mgr = PersonaManager::load();
        for mode in [BlackBoxMode::Clean, BlackBoxMode::Warm,
                     BlackBoxMode::Punch, BlackBoxMode::Air,
                     BlackBoxMode::Film, BlackBoxMode::Broadcast] {
            let (id, macros) = BlackBoxControl{mode}.to_intent();
            assert!(mgr.get(id).is_some(),
                "unknown persona: {}", id);
            assert!((0.0..=1.0).contains(&macros.warmth));
        }
    }

    #[test]
    fn orb_center_zero_delta() {
        let d = OrbPosition::center().to_macro_delta();
        assert_eq!(d.forwardness_delta, 0.0);
        assert_eq!(d.smoothness_delta,  0.0);
    }

    #[test]
    fn orb_delta_apply_clamped() {
        let mgr     = PersonaManager::load();
        let persona = mgr.get("clean_punch").unwrap();
        let macros  = MacroControls::default();
        let delta   = MacroDelta {
            forwardness_delta: 99.0,
            smoothness_delta:  99.0,
        };
        let result = delta.apply(&macros, persona);
        assert!(result.forwardness <= persona.macros.forwardness.max);
        assert!(result.smoothness  <= persona.macros.smoothness.max);
    }

    #[test]
    fn orb_resistance_center_zero() {
        assert_eq!(orb_resistance(&OrbPosition{x:0.0, y:0.0}), 0.0);
    }

    #[test]
    fn orb_resistance_wall_max() {
        assert!(orb_resistance(&OrbPosition{x:1.0, y:0.0}) >= 1.0);
    }

    #[test]
    fn medium_control_default_gain_match_on() {
        let m = MediumControl::new("warm_analog");
        assert!(m.gain_match);
        assert_eq!(m.orb, OrbPosition::center());
    }
}
