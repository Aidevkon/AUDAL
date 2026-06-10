// aether/semantic/mod.rs — S-007 Semantic Zones & Auto-Carve
// Authority: spec/locked/S-007_semantic_zones.md v1.0

pub mod resolver;
pub mod zone;

pub use resolver::SemanticZoneResolver;
pub use zone::{
    SemanticZone, ZoneAdjustment, ZoneAdjustments, CYMBAL_HARSH_CREST_THRESHOLD,
    SUB_RUMBLE_ENERGY_THRESHOLD, ZONE_GAIN_MAX_DB, ZONE_GAIN_MIN_DB, ZONE_OVERLAP_HZ_MIN,
    ZONE_Q_MAX, ZONE_Q_MIN,
};

pub mod editor;
pub use editor::{
    zone_color, ZoneEditor, ZoneEditorError, ZoneEditorState, ZoneEditorTier, ZoneVisual,
};
