// aether/personas/mod.rs — S-003 Persona Schema & Manager
// Authority: spec/locked/S-003_persona_schema.md v1.1
// Compile-time embedded TOML — no runtime file loading

pub mod config;
pub mod manager;

pub use config::{PersonaConfig, MacroHandleConfig, MacroHandles,
                 DspBaseConfig, ZonePriorities, CurveShape, ChaosProfile,
                 MacroControls, PersonaOverride, PERSONA_OVERRIDE_DELTA_MAX};
pub use manager::PersonaManager;
