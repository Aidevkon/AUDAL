# S-011b — Semantic Zone Editor

**Document:** `spec/locked/S-011b_semantic_zone_editor.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** UI
**Depends on:** S-007 (Semantic Zones & Auto-Carve)
**Used by:** S-011a (Multimodal Control Surface — Tier 3)
**Audit:** DeepSeek v0.1 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. N1: resolved band matching note. N2: find_components exposure note. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Semantic Zone Editor is the visual interface for S-007.
It allows engineers to see, edit, and understand how frequency
zones interact and resolve.

**Tier access:**
- Tier 2 (Medium): preset zones — view only
- Tier 3 (Pro): full zone editing + collision visualization

**One sentence:** A visual frequency editor that displays semantic zones
as colored bands, shows collision detection in real-time, and renders
the auto-carve result — all backed by S-007's deterministic resolver.

---

## 2. Constitutional Position

```
S-007 (SemanticZoneResolver) — deterministic core
    ↑ called by
ZoneEditor (S-011b) — UI layer only
    → SemanticZoneResolver::resolve(zones)
    → ZoneAdjustments → ProofLog → S-009
```

**Rules:**
- Editor is UI-only — calls S-007, never bypasses it
- All zone edits produce valid SemanticZone structs (bounded)
- Tier 2: read-only enforced by EditNotAllowed error
- Tier 3: full edit
- Collision visualization derived from S-007 — no independent logic
- No ML, no randomness

---

## 3. Zone Display Model

```
0 Hz                                                    20kHz
|---sub---|-------bass-------|---mid---|---pres---|--air--|

Zones rendered as colored horizontal bands:
  bass:         blue    (#4A90D9)
  presence:     green   (#7ED321)
  dialogue:     cyan    (#50E3C2)
  air:          purple  (#9B59B6)
  traffic:      orange  (#F5A623)
  cymbal_harsh: red     (#FF5252)
  sub_rumble:   dark    (#2C3E50)
```

---

## 4. Interface

### Types

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZoneVisual {
    pub id:           String,
    pub color:        String,      // CSS hex
    pub in_collision: bool,
    /// Nearest resolved band (from S-007 auto-carve).
    /// Matched by closest center_hz — works because resolved
    /// bands are derived from the same zone set.
    pub resolved:     Option<ZoneAdjustment>,
    pub zone:         SemanticZone,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZoneEditorState {
    pub zones:          Vec<SemanticZone>,
    pub resolved:       ZoneAdjustments,
    pub visuals:        Vec<ZoneVisual>,
    pub has_collisions: bool,
    pub tier:           ZoneEditorTier,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ZoneEditorTier {
    ViewOnly,  // Tier 2
    FullEdit,  // Tier 3
}
```

### ZoneEditor

```rust
pub struct ZoneEditor;

impl ZoneEditor {
    pub fn build(persona: &PersonaConfig, features: &StemFeatures,
                  tier: ZoneEditorTier) -> ZoneEditorState;

    pub fn recompute(state: &mut ZoneEditorState);

    /// Tier 3 only. Clamped to ZONE_GAIN_MIN_DB/MAX_DB.
    pub fn set_zone_gain(state: &mut ZoneEditorState,
                          zone_id: &str, gain_db: f32)
        -> Result<(), ZoneEditorError>;

    /// Tier 3 only. Clamped to [20, 20000] Hz.
    pub fn set_zone_center(state: &mut ZoneEditorState,
                            zone_id: &str, center_hz: f32)
        -> Result<(), ZoneEditorError>;

    /// Tier 3 only.
    pub fn toggle_zone(state: &mut ZoneEditorState, zone_id: &str)
        -> Result<(), ZoneEditorError>;

    pub fn reset(state: &mut ZoneEditorState,
                  persona: &PersonaConfig, features: &StemFeatures);
}

#[derive(Debug)]
pub enum ZoneEditorError {
    EditNotAllowed,
    ZoneNotFound(String),
    ValueClamped { field: String, original: f32, clamped: f32 },
}
```

---

## 5. Zone Colors

```rust
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
```

---

## 6. Collision Visualization

```rust
fn compute_visuals(zones: &[SemanticZone],
                   resolved: &ZoneAdjustments) -> Vec<ZoneVisual> {
    // find_components is exposed by S-007 (non-breaking addition)
    let components = find_components(zones);
    let colliding: std::collections::BTreeSet<usize> = components
        .iter()
        .filter(|c| c.len() > 1)
        .flat_map(|c| c.iter().copied())
        .collect();

    zones.iter().enumerate().map(|(i, zone)| {
        // Match to nearest resolved band by center_hz.
        // Assumes each zone maps to nearest band — works because
        // resolved bands are derived from the same zone set.
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
```

---

## 7. Build + Recompute

```rust
pub fn build(persona: &PersonaConfig, features: &StemFeatures,
              tier: ZoneEditorTier) -> ZoneEditorState {
    let zones    = SemanticZoneResolver::build_zones(persona, features);
    let resolved = SemanticZoneResolver::resolve(&zones);
    let visuals  = compute_visuals(&zones, &resolved);
    let has_collisions = visuals.iter().any(|v| v.in_collision);
    ZoneEditorState { zones, resolved, visuals, has_collisions, tier }
}

pub fn recompute(state: &mut ZoneEditorState) {
    state.resolved       = SemanticZoneResolver::resolve(&state.zones);
    state.visuals        = compute_visuals(&state.zones, &state.resolved);
    state.has_collisions = state.visuals.iter().any(|v| v.in_collision);
}
```

---

## 8. Edit Guards

```rust
fn check_editable(state: &ZoneEditorState) -> Result<(), ZoneEditorError> {
    if state.tier == ZoneEditorTier::ViewOnly {
        Err(ZoneEditorError::EditNotAllowed)
    } else { Ok(()) }
}

pub fn set_zone_gain(state: &mut ZoneEditorState,
                      zone_id: &str, gain_db: f32)
    -> Result<(), ZoneEditorError>
{
    check_editable(state)?;
    let zone = state.zones.iter_mut()
        .find(|z| z.id == zone_id)
        .ok_or_else(|| ZoneEditorError::ZoneNotFound(zone_id.into()))?;
    zone.gain_db = gain_db.clamp(ZONE_GAIN_MIN_DB, ZONE_GAIN_MAX_DB);
    Self::recompute(state);
    Ok(())
}
```

---

## 9. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Same zones → same visuals | ✅ S-007 pure function |
| Collision detection | ✅ From S-007 only |
| Zone edits bounded | ✅ Clamped to S-007 constants |
| Tier 2 read-only | ✅ EditNotAllowed |
| No ML, no randomness | ✅ Pure UI logic |

---

## 10. Performance Targets

| Metric | Target |
|--------|--------|
| `build()` | < 5ms |
| `recompute()` after edit | < 2ms |
| Visual render (8 zones) | < 16ms (60fps) |

---

## 11. Contract Tests

```rust
#[test]
fn editor_build_deterministic() {
    let p  = PersonaManager::get("warm_analog").unwrap();
    let f  = test_stem_features();
    let s1 = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
    let s2 = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
    assert_eq!(s1.zones.len(),    s2.zones.len());
    assert_eq!(s1.resolved,       s2.resolved);
    assert_eq!(s1.has_collisions, s2.has_collisions);
}
#[test]
fn editor_viewonly_edit_rejected() {
    let mut s = ZoneEditor::build(
        &PersonaManager::get("warm_analog").unwrap(),
        &test_stem_features(), ZoneEditorTier::ViewOnly);
    assert!(matches!(
        ZoneEditor::set_zone_gain(&mut s, "bass", 2.0),
        Err(ZoneEditorError::EditNotAllowed)
    ));
}
#[test]
fn editor_gain_clamped() {
    let mut s = ZoneEditor::build(
        &PersonaManager::get("warm_analog").unwrap(),
        &test_stem_features(), ZoneEditorTier::FullEdit);
    ZoneEditor::set_zone_gain(&mut s, "bass", 99.0).unwrap();
    let bass = s.zones.iter().find(|z| z.id == "bass").unwrap();
    assert!(bass.gain_db <= ZONE_GAIN_MAX_DB);
}
#[test]
fn editor_collision_reflects_s007() {
    let mut s = ZoneEditor::build(
        &PersonaManager::get("warm_analog").unwrap(),
        &test_stem_features(), ZoneEditorTier::FullEdit);
    ZoneEditor::set_zone_center(&mut s, "dialogue", 150.0).unwrap();
    let active = s.zones.iter().filter(|z| z.active).count();
    assert!(s.resolved.bands.len() <= active);
}
#[test]
fn editor_reset_restores_defaults() {
    let p = PersonaManager::get("warm_analog").unwrap();
    let f = test_stem_features();
    let mut s = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
    ZoneEditor::set_zone_gain(&mut s, "bass", 5.0).unwrap();
    ZoneEditor::reset(&mut s, &p, &f);
    let initial = ZoneEditor::build(&p, &f, ZoneEditorTier::FullEdit);
    assert_eq!(s.zones[0].gain_db, initial.zones[0].gain_db);
}
#[test]
fn editor_unknown_zone_error() {
    let mut s = ZoneEditor::build(
        &PersonaManager::get("warm_analog").unwrap(),
        &test_stem_features(), ZoneEditorTier::FullEdit);
    assert!(matches!(
        ZoneEditor::set_zone_gain(&mut s, "nonexistent", 1.0),
        Err(ZoneEditorError::ZoneNotFound(_))
    ));
}
#[test]
fn zone_colors_defined_for_all_builtin() {
    for id in ["bass","presence","dialogue","air",
               "traffic","music_bed","cymbal_harsh","sub_rumble"] {
        assert_ne!(zone_color(id), "#95A5A6");
    }
}
#[test]
fn editor_state_serializable() {
    let s = ZoneEditor::build(
        &PersonaManager::get("warm_analog").unwrap(),
        &test_stem_features(), ZoneEditorTier::FullEdit);
    let _: ZoneEditorState = serde_json::from_str(
        &serde_json::to_string(&s).unwrap()).unwrap();
}
```

---

## 12. Error Handling

| Condition | Behavior |
|-----------|----------|
| Edit in ViewOnly | `Err(EditNotAllowed)` |
| Unknown zone ID | `Err(ZoneNotFound)` |
| Value out of bounds | Clamped, `Ok(())` |
| Empty zones | Empty canvas |

---

## 13. Implementation Path

```
apps/stillair/cockpit-dioxus/src/components/zone_editor/
├── mod.rs      ← ZoneEditor, ZoneEditorState
├── canvas.rs   ← frequency canvas rendering
├── visuals.rs  ← ZoneVisual, zone_color()
└── controls.rs ← edit controls (Tier 3 only)
```

**Reuses S-007:** `build_zones()`, `resolve()`, `find_components()`
(S-007 exposes `find_components` as public — non-breaking addition).

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-011b_semantic_zone_editor.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*Visual layer on top of deterministic core.*
*Tier 2 sees. Tier 3 edits. S-007 resolves. Always.*
