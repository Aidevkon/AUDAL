// aether/personas/config.rs — Persona types
// Authority: spec/locked/S-003_persona_schema.md v1.1

/// Maximum delta for any PersonaOverride field (session-only).
/// Applied via PersonaManager::apply_override().
pub const PERSONA_OVERRIDE_DELTA_MAX: f32 = 0.5;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CurveShape {
    Linear,
    Log,
    Exp,
}

/// Per-macro handle configuration.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MacroHandleConfig {
    pub default: f32,
    pub min:     f32,
    pub max:     f32,
    pub curve:   CurveShape,
}

/// Base DSP parameters for a persona.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspBaseConfig {
    pub low_shelf_gain_db:  f32,
    pub low_shelf_freq_hz:  f32,
    pub high_shelf_gain_db: f32,
    pub high_shelf_freq_hz: f32,
    pub comp_threshold_db:  f32,
    pub comp_ratio:         f32,
    pub comp_attack_ms:     f32,
    pub comp_release_ms:    f32,
    pub saturation_drive:   f32,
    pub saturation_mix:     f32,
    pub stereo_width:       f32,
}

/// Zone priority weights [1, 10].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ZonePriorities {
    pub dialogue: u8,
    pub bass:     u8,
    pub air:      u8,
    pub presence: u8,
}

/// Per-persona chaos depth signature (S-006).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChaosProfile {
    pub width_depth:   f32,
    pub drive_depth:   f32,
    pub release_depth: f32,
    pub attack_depth:  f32,
    pub air_depth:     f32,
}

/// Macro handle grouping for a persona.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MacroHandles {
    pub warmth:      MacroHandleConfig,
    pub punch:       MacroHandleConfig,
    pub forwardness: MacroHandleConfig,
    pub smoothness:  MacroHandleConfig,
}

/// Full persona configuration (parsed from TOML).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PersonaConfig {
    pub id:              String,
    #[serde(rename = "display_name")]
    pub name:            String,
    pub description:     String,
    pub chaos_intensity: f32,
    pub macros:          MacroHandles,
    pub dsp_base:        DspBaseConfig,
    pub zone_priorities: ZonePriorities,
    pub chaos:           ChaosProfile,
}

/// Active macro control values [0.0, 1.0].
/// Clamped to each handle's min/max before use.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MacroControls {
    pub warmth:      f32,
    pub forwardness: f32,
    pub punch:       f32,
    pub smoothness:  f32,
}

impl MacroControls {
    pub fn default_for(persona: &PersonaConfig) -> Self {
        Self {
            warmth:      persona.macros.warmth.default,
            punch:       persona.macros.punch.default,
            forwardness: persona.macros.forwardness.default,
            smoothness:  persona.macros.smoothness.default,
        }
    }
}

impl Default for MacroControls {
    fn default() -> Self {
        Self { warmth:0.5, punch:0.5, forwardness:0.5, smoothness:0.5 }
    }
}

/// Session-only persona override (from S-008 ATE or S-011a UI).
/// Applied on top of PersonaConfig — original TOML never modified.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PersonaOverride {
    pub warmth_delta:      f32,
    pub punch_delta:       f32,
    pub forwardness_delta: f32,
    pub smoothness_delta:  f32,
}

impl Default for PersonaOverride {
    fn default() -> Self {
        Self { warmth_delta:0.0, punch_delta:0.0,
               forwardness_delta:0.0, smoothness_delta:0.0 }
    }
}
