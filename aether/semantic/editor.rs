// aether/semantic/editor.rs — ZoneEditor pure logic
// Authority: spec/locked/S-011b_semantic_zone_editor.md v1.0
// Pure view-model — no UI framework imports.
// Backed by SemanticZoneResolver (S-007).

use lineos_types::analysis::StemFeatures;
use crate::personas::config::PersonaConfig;
use super::zone::*;
use super::resolver::SemanticZoneResolver;

/// Access tier — determines edit capability.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ZoneEditorTier {
    /// Tier 2: view only, preset zones
    ViewOnly,
    /// Tier 3: full edit
    FullEdit,
}

/// Visual representation of a zone for rendering.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZoneVisual {
    pub id:           String,
    /// CSS hex color string (e.g. "#4A90D9")
    /// Pure static data — deterministic, no UI framework.
    pub color:        String,
    /// True if zone is in collision with at least one other zone
    pub in_collision: bool,
    /// Nearest resolved band from S-007 auto-carve
    /// Matched by closest center_hz (S-007 guarantees sort)
    pub resolved:     Option<ZoneAdjustment>,
    pub zone:         SemanticZone,
}

/// Full editor state — deterministic transformation of zones
/// into visual-ready data. Backed by S-007 resolver.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZoneEditorState {
    pub zones:          Vec<SemanticZone>,
    pub resolved:       ZoneAdjustments,
    pub visuals:        Vec<ZoneVisual>,
    pub has_collisions: bool,
    pub tier:           ZoneEditorTier,
}

/// Editor errors.
#[derive(Debug)]
pub enum ZoneEditorError {
    /// Attempted edit in ViewOnly tier
    EditNotAllowed,
    /// Zone ID not found
    ZoneNotFound(String),
}

/// Map zone ID to CSS hex color string.
/// Pure function — deterministic, no side effects.
pub fn zone_color(id: &str) -> &'static str {
    match id {
        "bass"         => "#4A90D9",
        "presence"     => "#7ED321",
        "dialogue"     => "#50E3C2",
        "air"          => "#9B59B6",
        "traffic"      => "#F5A623",
        "music_bed"    => "#D0021B",
        "cymbal_harsh" => "#FF5252",
        "sub_rumble"   => "#2C3E50",
        _              => "#95A5A6",
    }
}

pub struct ZoneEditor;

impl ZoneEditor {
    /// Build initial editor state from persona and stem features.
    pub fn build(persona:  &PersonaConfig,
                 features: &StemFeatures,
                 tier:     ZoneEditorTier) -> ZoneEditorState {
        let zones    = SemanticZoneResolver::build_zones(persona, features, None);
        let resolved = SemanticZoneResolver::resolve(&zones);
        let visuals  = compute_visuals(&zones, &resolved);
        let has_collisions = visuals.iter().any(|v| v.in_collision);
        ZoneEditorState { zones, resolved, visuals, has_collisions, tier }
    }

    /// Recompute visuals and resolved bands after any zone edit.
    /// Calls S-007 resolver — deterministic.
    pub fn recompute(state: &mut ZoneEditorState) {
        state.resolved       = SemanticZoneResolver::resolve(&state.zones);
        state.visuals        = compute_visuals(&state.zones,
                                               &state.resolved);
        state.has_collisions = state.visuals.iter().any(|v| v.in_collision);
    }

    /// Edit a zone's gain (Tier 3 only).
    /// Clamped to ZONE_GAIN_MIN_DB / ZONE_GAIN_MAX_DB.
    pub fn set_zone_gain(state:   &mut ZoneEditorState,
                          zone_id: &str,
                          gain_db: f32)
        -> Result<(), ZoneEditorError>
    {
        check_editable(state)?;
        let zone = find_zone_mut(state, zone_id)?;
        zone.gain_db = gain_db.clamp(ZONE_GAIN_MIN_DB, ZONE_GAIN_MAX_DB);
        Self::recompute(state);
        Ok(())
    }

    /// Edit a zone's center frequency (Tier 3 only).
    /// Clamped to [20, 20000] Hz.
    pub fn set_zone_center(state:     &mut ZoneEditorState,
                            zone_id:   &str,
                            center_hz: f32)
        -> Result<(), ZoneEditorError>
    {
        check_editable(state)?;
        let zone = find_zone_mut(state, zone_id)?;
        zone.center_hz = center_hz.clamp(20.0, 20_000.0);
        Self::recompute(state);
        Ok(())
    }

    /// Toggle a zone active/inactive (Tier 3 only).
    pub fn toggle_zone(state:   &mut ZoneEditorState,
                        zone_id: &str)
        -> Result<(), ZoneEditorError>
    {
        check_editable(state)?;
        let zone = find_zone_mut(state, zone_id)?;
        zone.active = !zone.active;
        Self::recompute(state);
        Ok(())
    }

    /// Reset all zones to persona defaults.
    pub fn reset(state:    &mut ZoneEditorState,
                  persona:  &PersonaConfig,
                  features: &StemFeatures) {
        let fresh = Self::build(persona, features, state.tier.clone());
        *state = fresh;
    }
}

// ── private helpers ───────────────────────────────────────────────

fn check_editable(state: &ZoneEditorState)
    -> Result<(), ZoneEditorError>
{
    if state.tier == ZoneEditorTier::ViewOnly {
        Err(ZoneEditorError::EditNotAllowed)
    } else {
        Ok(())
    }
}

fn find_zone_mut<'a>(state:   &'a mut ZoneEditorState,
                      zone_id: &str)
    -> Result<&'a mut SemanticZone, ZoneEditorError>
{
    state.zones.iter_mut()
        .find(|z| z.id == zone_id)
        .ok_or_else(|| ZoneEditorError::ZoneNotFound(zone_id.into()))
}

fn compute_visuals(zones:    &[SemanticZone],
                   resolved: &ZoneAdjustments) -> Vec<ZoneVisual>
{
    use std::collections::BTreeSet;

    let components = SemanticZoneResolver::find_components(zones);
    let colliding: BTreeSet<usize> = components.iter()
        .filter(|c| c.len() > 1)
        .flat_map(|c| c.iter().copied())
        .collect();

    zones.iter().enumerate().map(|(i, zone)| {
        // Match to nearest resolved band by center_hz.
        // Works because resolved bands derive from same zone set.
        let resolved_band = resolved.bands.iter()
            .min_by(|a, b| {
                (a.center_hz - zone.center_hz).abs()
                    .total_cmp(&(b.center_hz - zone.center_hz).abs())
            })
            .cloned();

        ZoneVisual {
            id:           zone.id.clone(),
            color:        zone_color(&zone.id).into(),
            in_collision: colliding.contains(&i),
            resolved:     resolved_band,
            zone:         zone.clone(),
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::analysis::{StemFeatures, StemMetrics, MixMetrics};
    use crate::personas::manager::PersonaManager;

    fn test_stem_features() -> StemFeatures {
        StemFeatures {
            bass:      StemMetrics::default(),
            harmonics: StemMetrics::default(),
            voice:     StemMetrics::default(),
            drums:     StemMetrics::default(),
            ambience:  StemMetrics::default(),
            mix:       MixMetrics::default(),
        }
    }

    fn default_persona() -> PersonaConfig {
        PersonaManager::load().default_persona().clone()
    }

    #[test]
    fn editor_build_deterministic() {
        let p  = default_persona();
        let f  = test_stem_features();
        let s1 = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
        let s2 = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
        assert_eq!(s1.zones.len(),    s2.zones.len());
        assert_eq!(s1.has_collisions, s2.has_collisions);
    }

    #[test]
    fn editor_viewonly_edit_rejected() {
        let p   = default_persona();
        let f   = test_stem_features();
        let mut s = ZoneEditor::build(&p, &f, ZoneEditorTier::ViewOnly);
        assert!(matches!(
            ZoneEditor::set_zone_gain(&mut s, "bass", 2.0),
            Err(ZoneEditorError::EditNotAllowed)
        ));
    }

    #[test]
    fn editor_gain_clamped() {
        let p   = default_persona();
        let f   = test_stem_features();
        let mut s = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
        ZoneEditor::set_zone_gain(&mut s, "bass", 99.0).unwrap();
        let bass = s.zones.iter().find(|z| z.id == "bass").unwrap();
        assert!(bass.gain_db <= ZONE_GAIN_MAX_DB);
    }

    #[test]
    fn editor_collision_reflects_s007() {
        let p   = default_persona();
        let f   = test_stem_features();
        let mut s = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
        // Force dialogue to overlap with bass
        ZoneEditor::set_zone_center(&mut s, "dialogue", 150.0).unwrap();
        let active_count = s.zones.iter()
            .filter(|z| z.active).count();
        assert!(s.resolved.bands.len() <= active_count);
    }

    #[test]
    fn editor_reset_restores_defaults() {
        let p   = default_persona();
        let f   = test_stem_features();
        let mut s = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
        ZoneEditor::set_zone_gain(&mut s, "bass", 5.0).unwrap();
        ZoneEditor::reset(&mut s, &p, &f);
        let initial = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
        assert_eq!(s.zones[0].gain_db, initial.zones[0].gain_db);
    }

    #[test]
    fn editor_unknown_zone_error() {
        let p   = default_persona();
        let f   = test_stem_features();
        let mut s = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
        assert!(matches!(
            ZoneEditor::set_zone_gain(&mut s, "nonexistent", 1.0),
            Err(ZoneEditorError::ZoneNotFound(_))
        ));
    }

    #[test]
    fn zone_colors_defined_for_all_builtin() {
        for id in ["bass","presence","dialogue","air",
                   "traffic","music_bed","cymbal_harsh","sub_rumble"] {
            assert_ne!(zone_color(id), "#95A5A6",
                "Missing color for zone: {}", id);
        }
    }

    #[test]
    fn editor_state_serializable() {
        let p = default_persona();
        let f = test_stem_features();
        let s = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
        let json = serde_json::to_string(&s).unwrap();
        let _: ZoneEditorState = serde_json::from_str(&json).unwrap();
    }
}
