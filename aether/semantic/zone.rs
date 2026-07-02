// aether/semantic/zone.rs — SemanticZone types and constants
// Authority: spec/locked/S-007_semantic_zones.md v1.0

pub const ZONE_GAIN_MIN_DB: f32 = -9.0;
pub const ZONE_GAIN_MAX_DB: f32 = 6.0;
pub const ZONE_Q_MIN: f32 = 0.3;
pub const ZONE_Q_MAX: f32 = 4.0;
pub const ZONE_PRIORITY_MIN: u8 = 1;
pub const ZONE_PRIORITY_MAX: u8 = 10;
pub const ZONE_OVERLAP_HZ_MIN: f32 = 10.0;
pub const CYMBAL_HARSH_CREST_THRESHOLD: f32 = 15.0;
pub const SUB_RUMBLE_ENERGY_THRESHOLD: f32 = 0.35;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SemanticZone {
    pub id: String,
    pub center_hz: f32,
    pub bandwidth_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    pub priority: u8,
    pub active: bool,
}

impl SemanticZone {
    pub fn freq_low(&self) -> f32 {
        self.center_hz - self.bandwidth_hz / 2.0
    }
    pub fn freq_high(&self) -> f32 {
        self.center_hz + self.bandwidth_hz / 2.0
    }
    pub fn overlaps(&self, other: &SemanticZone) -> bool {
        let overlap =
            self.freq_high().min(other.freq_high()) - self.freq_low().max(other.freq_low());
        overlap >= ZONE_OVERLAP_HZ_MIN
    }
}

/// Provenance of an EQ adjustment — which subsystem
/// produced it. Enables full traceability in the
/// ProofLog / certificate ("in dark, not hidden").
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum EqSource {
    /// Produced by the SemanticZoneResolver
    /// (persona/flavour-driven surgical carving).
    #[default]
    Semantic,
    /// Produced by the ReferenceResolver
    /// (LTASS reference shape correction).
    Reference,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ZoneAdjustment {
    pub center_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    /// Which subsystem produced this band.
    #[serde(default)]
    pub source: EqSource,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ZoneAdjustments {
    /// Sorted by center_hz ascending — deterministic order
    pub bands: Vec<ZoneAdjustment>,
}

impl ZoneAdjustments {
    pub fn empty() -> Self {
        Self { bands: vec![] }
    }
}
