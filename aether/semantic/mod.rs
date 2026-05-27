// aether/semantic/mod.rs — S-007 Semantic Zones & Auto-Carve
// Authority: spec/locked/S-007_semantic_zones.md v1.0

pub mod zone;
pub mod resolver;

pub use zone::{SemanticZone, ZoneAdjustment, ZoneAdjustments,
               ZONE_GAIN_MIN_DB, ZONE_GAIN_MAX_DB,
               ZONE_Q_MIN, ZONE_Q_MAX,
               ZONE_OVERLAP_HZ_MIN,
               CYMBAL_HARSH_CREST_THRESHOLD,
               SUB_RUMBLE_ENERGY_THRESHOLD};
pub use resolver::SemanticZoneResolver;
