// aether/personas/manager.rs — PersonaManager
// Authority: spec/locked/S-003_persona_schema.md v1.1
// Compile-time embedded TOML via include_str!()
// No runtime file I/O.

use super::config::{MacroControls, PersonaConfig, PersonaOverride, PERSONA_OVERRIDE_DELTA_MAX};

/// Raw TOML embedded at compile time.
const PERSONAS_TOML: &str = include_str!("../../config/personas.toml");

/// Helper struct to deserialize the [[personas]] array.
#[derive(serde::Deserialize)]
struct PersonasFile {
    personas: Vec<PersonaConfig>,
}

/// Singleton persona registry — parsed once at startup.
pub struct PersonaManager {
    personas: Vec<PersonaConfig>,
}

impl PersonaManager {
    /// Parse personas.toml at startup.
    /// Panics if TOML is invalid — this is a compile-time guarantee.
    pub fn load() -> Self {
        let file: PersonasFile = toml::from_str(PERSONAS_TOML)
            .expect("personas.toml must be valid — constitutional invariant");
        Self {
            personas: file.personas,
        }
    }

    /// Get persona by ID.
    pub fn get(&self, id: &str) -> Option<&PersonaConfig> {
        self.personas.iter().find(|p| p.id == id)
    }

    /// List all persona IDs.
    pub fn list(&self) -> Vec<&str> {
        self.personas.iter().map(|p| p.id.as_str()).collect()
    }

    /// Default persona (first in list = warm_analog).
    pub fn default_persona(&self) -> &PersonaConfig {
        &self.personas[0]
    }

    /// Apply a session-only PersonaOverride to MacroControls.
    /// Clamps result to each handle's [min, max].
    /// Original PersonaConfig is never modified.
    pub fn apply_override(
        persona: &PersonaConfig,
        macros: &MacroControls,
        override_: &PersonaOverride,
    ) -> MacroControls {
        let clamp = |v: f32, min: f32, max: f32| v.clamp(min, max);
        let delta_clamp = |d: f32| d.clamp(-PERSONA_OVERRIDE_DELTA_MAX, PERSONA_OVERRIDE_DELTA_MAX);

        MacroControls {
            tone: clamp(
                macros.tone + delta_clamp(override_.tone_delta),
                persona.macros.tone.min,
                persona.macros.tone.max,
            ),
            dynamics: clamp(
                macros.dynamics + delta_clamp(override_.dynamics_delta),
                persona.macros.dynamics.min,
                persona.macros.dynamics.max,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personas_load_four() {
        let mgr = PersonaManager::load();
        assert_eq!(mgr.list().len(), 4);
    }

    #[test]
    fn personas_default_is_warm_analog() {
        let mgr = PersonaManager::load();
        assert_eq!(mgr.default_persona().id, "warm_analog");
    }

    #[test]
    fn personas_get_all_four() {
        let mgr = PersonaManager::load();
        for id in [
            "warm_analog",
            "clean_punch",
            "hybrid_hifi",
            "cinematic_wide",
        ] {
            assert!(mgr.get(id).is_some(), "missing persona: {}", id);
        }
    }

    #[test]
    fn personas_override_clamped() {
        let mgr = PersonaManager::load();
        let persona = mgr.get("warm_analog").unwrap();
        let macros = MacroControls::default_for(persona);
        let ov = PersonaOverride {
            tone_delta: 99.0,
            dynamics_delta: -99.0,
        };
        let result = PersonaManager::apply_override(persona, &macros, &ov);
        assert!(result.tone <= persona.macros.tone.max);
        assert!(result.dynamics >= persona.macros.dynamics.min);
    }

    #[test]
    fn personas_chaos_profiles_bounded() {
        let mgr = PersonaManager::load();
        for id in mgr.list() {
            let p = mgr.get(id).unwrap();
            for depth in [
                p.chaos.width_depth,
                p.chaos.drive_depth,
                p.chaos.release_depth,
                p.chaos.attack_depth,
                p.chaos.air_depth,
            ] {
                assert!(
                    depth >= 0.0 && depth <= 1.0,
                    "chaos depth out of bounds in {}: {}",
                    id,
                    depth
                );
            }
        }
    }

    #[test]
    fn personas_serializable() {
        let mgr = PersonaManager::load();
        let p = mgr.default_persona();
        let json = serde_json::to_string(p).unwrap();
        let p2: PersonaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(p.id, p2.id);
    }
}
