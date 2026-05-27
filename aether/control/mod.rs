// aether/control/mod.rs — S-011a Control Surface (pure logic)
// Authority: spec/locked/S-011a_multimodal_control.md v1.0

pub mod types;

pub use types::{BlackBoxMode, BlackBoxControl, OrbPosition,
                MacroDelta, MediumControl, orb_resistance};
