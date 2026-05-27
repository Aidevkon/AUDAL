# S-007 — Semantic Zones & Auto-Carve

**Document:** `spec/locked/S-007_semantic_zones.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Aether
**Depends on:** S-002 (Stem Feature Analyzer), S-003 (Persona Schema), S-005 (Macro → Micro Mapping)
**Used by:** S-006 (Chaos Engine), S-009 (Integration Firewall)
**Execution:** Before S-006 (Chaos) per RFC-002 data flow
**Audit:** DeepSeek v0.1 → REJECT · v0.2 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. |
| 0.2 | 2026-05-27 | C1: union-find clustering. C2: BTreeMap. M1: weighted average. M3+M4: clarifications. +3 tests. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

Zone-based EQ intelligence. The engineer manipulates named sound objects
(Dialogue, Traffic, Music Bed) and the system resolves conflicts
deterministically.

**Core principle:** Deterministic core, creative semantics.
Same zones + same priorities → same carved EQ, always.
Creative element lives in S-006 (Chaos) — NOT in the carve.

**Relationship to S-005:** `SemanticZoneResolver` does not modify
`MicroDelta`. It produces `ZoneAdjustments` independently.
S-009 combines `MicroDelta` + `ZoneAdjustments` into final `DspConfig`.

**One sentence:** Given semantic zones with frequency targets and
priorities, produce a deterministic, conflict-free set of EQ bands —
one per connected component of overlapping zones.

---

## 2. Constitutional Position

```
StemFeatures (S-002) + PersonaConfig (S-003)
    ↓
SemanticZoneResolver (S-007)
    ↓ ZoneAdjustments (typed, sorted by center_hz)
    ↓ schema validation deferred to S-009 caller
    ↓
S-006 (Chaos) → S-009 (Integration Firewall)

Note: MicroDelta (S-005) not used here.
      Zone adjustments = additive EQ bands.
      Combined with MicroDelta in S-009.
```

**Rules:**
- No ML, no randomness — pure deterministic resolver
- Same zones + same priorities → same output
- Connected-component clustering — one band per conflicting region
- BTreeMap + sorted Vec — no HashSet
- Priority-weighted average across entire component
- Persona-aware zone priorities (S-003)
- Schema validation by S-009 caller

---

## 3. Zone Taxonomy

| Zone | Center Hz | Bandwidth Hz | Default Priority | Character |
|------|-----------|-------------|-----------------|-----------|
| `dialogue` | 3000 | 3000 | persona | Vocal presence |
| `bass` | 120 | 190 | persona | Low-end body |
| `air` | 14000 | 10000 | persona | Shimmer |
| `presence` | 1800 | 2000 | persona | Midrange warmth |
| `traffic` | 3000 | 2000 | 3 | Background noise |
| `music_bed` | 350 | 300 | 3 | Background music |
| `cymbal_harsh` | 9000 | 5000 | 4 | Harshness |
| `sub_rumble` | 40 | 40 | 5 | Sub stability |

---

## 4. Interface

### Constants

```rust
pub const ZONE_GAIN_MIN_DB:              f32 = -9.0;
pub const ZONE_GAIN_MAX_DB:              f32 =  6.0;
pub const ZONE_Q_MIN:                    f32 =  0.3;
pub const ZONE_Q_MAX:                    f32 =  4.0;
pub const ZONE_PRIORITY_MIN:             u8  =  1;
pub const ZONE_PRIORITY_MAX:             u8  =  10;
pub const ZONE_OVERLAP_HZ_MIN:           f32 =  10.0;
pub const CYMBAL_HARSH_CREST_THRESHOLD:  f32 =  15.0; // dB, tunable
pub const SUB_RUMBLE_ENERGY_THRESHOLD:   f32 =  0.35;
```

### SemanticZone

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SemanticZone {
    pub id:           String,
    pub center_hz:    f32,
    pub bandwidth_hz: f32,
    pub gain_db:      f32,
    pub q:            f32,
    pub priority:     u8,
    pub active:       bool,
}

impl SemanticZone {
    pub fn freq_low(&self)  -> f32 { self.center_hz - self.bandwidth_hz / 2.0 }
    pub fn freq_high(&self) -> f32 { self.center_hz + self.bandwidth_hz / 2.0 }
    pub fn overlaps(&self, other: &SemanticZone) -> bool {
        let overlap = self.freq_high().min(other.freq_high())
                    - self.freq_low().max(other.freq_low());
        overlap >= ZONE_OVERLAP_HZ_MIN
    }
}
```

### ZoneAdjustment / ZoneAdjustments

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ZoneAdjustment {
    pub center_hz: f32,
    pub gain_db:   f32,
    pub q:         f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ZoneAdjustments {
    /// Sorted by center_hz ascending — deterministic order
    pub bands: Vec<ZoneAdjustment>,
}

impl ZoneAdjustments {
    pub fn empty() -> Self { Self { bands: vec![] } }
}
```

### SemanticZoneResolver

```rust
pub struct SemanticZoneResolver;

impl SemanticZoneResolver {
    pub fn build_zones(persona: &PersonaConfig,
                       features: &StemFeatures) -> Vec<SemanticZone>;
    pub fn resolve(zones: &[SemanticZone]) -> ZoneAdjustments;
    pub fn auto_carve(persona: &PersonaConfig,
                      features: &StemFeatures) -> ZoneAdjustments;
}
```

---

## 5. Zone Building

```rust
pub fn build_zones(persona: &PersonaConfig,
                   features: &StemFeatures) -> Vec<SemanticZone> {
    let mut zones = vec![];

    zones.push(SemanticZone {
        id: "dialogue".into(), center_hz: 3000.0, bandwidth_hz: 3000.0,
        gain_db: zone_gain_from_priority(
            persona.zone_priorities.dialogue, 0.0, 2.0),
        q: 0.7, priority: persona.zone_priorities.dialogue, active: true,
    });
    zones.push(SemanticZone {
        id: "bass".into(), center_hz: 120.0, bandwidth_hz: 190.0,
        gain_db: zone_gain_from_priority(
            persona.zone_priorities.bass, 0.0, 2.5),
        q: 0.5, priority: persona.zone_priorities.bass, active: true,
    });
    zones.push(SemanticZone {
        id: "air".into(), center_hz: 14000.0, bandwidth_hz: 10000.0,
        gain_db: zone_gain_from_priority(
            persona.zone_priorities.air, 0.0, 2.0),
        q: 0.4, priority: persona.zone_priorities.air, active: true,
    });

    if features.vocals.spectral_crest_factor > CYMBAL_HARSH_CREST_THRESHOLD {
        zones.push(SemanticZone {
            id: "cymbal_harsh".into(), center_hz: 9000.0,
            bandwidth_hz: 5000.0, gain_db: -1.5,
            q: 0.8, priority: 4, active: true,
        });
    }
    if features.mix.stem_energy_ratios[0] > SUB_RUMBLE_ENERGY_THRESHOLD {
        zones.push(SemanticZone {
            id: "sub_rumble".into(), center_hz: 40.0,
            bandwidth_hz: 40.0, gain_db: -1.0,
            q: 1.0, priority: 5, active: true,
        });
    }
    zones
}

fn zone_gain_from_priority(priority: u8, min_gain: f32,
                            max_gain: f32) -> f32 {
    let t = (priority as f32 - 1.0) / 9.0;
    min_gain + t * (max_gain - min_gain)
}
```

---

## 6. Connected-Component Clustering (Union-Find)

```rust
fn find_components(zones: &[SemanticZone]) -> Vec<Vec<usize>> {
    let n = zones.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        if parent[i] != i { parent[i] = find(parent, parent[i]); }
        parent[i]
    }
    fn union(parent: &mut Vec<usize>, i: usize, j: usize) {
        let (ri, rj) = (find(parent, i), find(parent, j));
        if ri != rj { parent[rj] = ri; }
    }

    for i in 0..n {
        for j in (i+1)..n {
            if zones[i].active && zones[j].active
               && zones[i].overlaps(&zones[j]) {
                union(&mut parent, i, j);
            }
        }
    }

    let mut components: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for i in 0..n {
        if zones[i].active {
            let root = find(&mut parent, i);
            components.entry(root).or_default().push(i);
        }
    }

    let mut result: Vec<Vec<usize>> = components.into_values()
        .map(|mut v| { v.sort_unstable(); v })
        .collect();
    result.sort_by_key(|c| c[0]);
    result
}
```

---

## 7. Component Resolution (Priority-Weighted Average)

```rust
fn resolve_component(zones: &[SemanticZone],
                     indices: &[usize]) -> ZoneAdjustment {
    if indices.len() == 1 {
        let z = &zones[indices[0]];
        return ZoneAdjustment {
            center_hz: z.center_hz,
            gain_db:   z.gain_db.clamp(ZONE_GAIN_MIN_DB, ZONE_GAIN_MAX_DB),
            q:         z.q,
        };
    }

    let total_w: f32 = indices.iter()
        .map(|&i| zones[i].priority as f32).sum();

    let center_hz = indices.iter()
        .map(|&i| zones[i].priority as f32 * zones[i].center_hz)
        .sum::<f32>() / total_w;

    let gain_db = (indices.iter()
        .map(|&i| zones[i].priority as f32 * zones[i].gain_db)
        .sum::<f32>() / total_w)
        .clamp(ZONE_GAIN_MIN_DB, ZONE_GAIN_MAX_DB);

    let q = indices.iter()
        .map(|&i| zones[i].q)
        .fold(0.0_f32, f32::max)
        .clamp(ZONE_Q_MIN, ZONE_Q_MAX);

    ZoneAdjustment { center_hz, gain_db, q }
}
```

---

## 8. Auto-Carve Pipeline

```rust
pub fn resolve(zones: &[SemanticZone]) -> ZoneAdjustments {
    if zones.iter().all(|z| !z.active) {
        return ZoneAdjustments::empty();
    }
    let components = find_components(zones);
    let mut bands: Vec<ZoneAdjustment> = components.iter()
        .map(|indices| resolve_component(zones, indices))
        .collect();
    // Sort by center_hz — deterministic output order
    bands.sort_by(|a, b| a.center_hz.total_cmp(&b.center_hz));
    ZoneAdjustments { bands }
}

pub fn auto_carve(persona: &PersonaConfig,
                  features: &StemFeatures) -> ZoneAdjustments {
    Self::resolve(&Self::build_zones(persona, features))
}
```

**3-zone example:**
```
Zone A (+2dB @3k, p=6) + Zone B (-3dB @3k, p=5) + Zone C (+1dB @2.5k, p=4)
    → all overlap → one component
    → total_w=15, gain=(12-15+4)/15≈0.07dB
    → ONE ZoneAdjustment: 0.07dB @ weighted_center
```

---

## 9. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Same zones → same output | ✅ Pure function |
| No HashSet | ✅ BTreeMap + sorted Vec |
| One band per component | ✅ Union-find |
| Output order stable | ✅ Sorted by center_hz |
| No ML, no randomness | ✅ Pure rule-based |
| Creative element outside | ✅ Chaos in S-006 |

---

## 10. Performance Targets

| Metric | Target |
|--------|--------|
| `resolve()` (8 zones) | < 100μs |
| `build_zones()` | < 50μs |
| Memory | < 10 KB |

---

## 11. Contract Tests

```rust
#[test]
fn zones_resolve_deterministic() {
    let z = test_zones();
    assert_eq!(SemanticZoneResolver::resolve(&z),
               SemanticZoneResolver::resolve(&z));
}
#[test]
fn zones_no_collision_passthrough() {
    let z = vec![make_zone("bass",120.0,100.0,1.5,0.5,7),
                 make_zone("air",14000.0,8000.0,1.0,0.4,8)];
    assert_eq!(SemanticZoneResolver::resolve(&z).bands.len(), 2);
}
#[test]
fn zones_two_overlapping_one_band() {
    let z = vec![make_zone("dialogue",3000.0,3000.0,2.0,0.7,6),
                 make_zone("traffic",3000.0,2000.0,-3.0,0.8,3)];
    assert_eq!(SemanticZoneResolver::resolve(&z).bands.len(), 1);
}
#[test]
fn zones_three_overlapping_one_band() {
    let z = vec![make_zone("a",3000.0,3000.0,2.0,0.7,6),
                 make_zone("b",3000.0,2000.0,-3.0,0.8,5),
                 make_zone("c",2500.0,2000.0,1.0,0.6,4)];
    assert_eq!(SemanticZoneResolver::resolve(&z).bands.len(), 1);
}
#[test]
fn zones_three_weighted_average() {
    let z = vec![make_zone("a",3000.0,3000.0,2.0,0.7,6),
                 make_zone("b",3000.0,2000.0,-3.0,0.8,5),
                 make_zone("c",2500.0,2000.0,1.0,0.6,4)];
    let expected = (6.0*2.0 + 5.0*(-3.0) + 4.0*1.0) / 15.0;
    let r = SemanticZoneResolver::resolve(&z);
    assert!((r.bands[0].gain_db - expected).abs() < 1e-4);
}
#[test]
fn zones_two_components_correct_count() {
    let z = vec![make_zone("bass",120.0,100.0,1.5,0.5,7),
                 make_zone("dialogue",3000.0,3000.0,2.0,0.7,6),
                 make_zone("traffic",3000.0,2000.0,-3.0,0.8,3)];
    assert_eq!(SemanticZoneResolver::resolve(&z).bands.len(), 2);
}
#[test]
fn zones_output_sorted_by_center_hz() {
    let z = vec![make_zone("air",14000.0,8000.0,1.0,0.4,8),
                 make_zone("bass",120.0,100.0,1.5,0.5,7)];
    let r = SemanticZoneResolver::resolve(&z);
    assert!(r.bands[0].center_hz < r.bands[1].center_hz);
}
#[test]
fn zones_gain_bounds_respected() {
    let z = vec![make_zone("a",1000.0,500.0,99.0,1.0,5)];
    let r = SemanticZoneResolver::resolve(&z);
    assert!(r.bands[0].gain_db <= ZONE_GAIN_MAX_DB);
    assert!(r.bands[0].gain_db >= ZONE_GAIN_MIN_DB);
}
#[test]
fn zones_inactive_excluded() {
    let mut z = vec![make_zone("a",3000.0,2000.0,2.0,0.7,6),
                     make_zone("b",120.0,100.0,1.5,0.5,7)];
    z[0].active = false;
    assert_eq!(SemanticZoneResolver::resolve(&z).bands.len(), 1);
}
#[test]
fn zones_auto_carve_deterministic() {
    let p = PersonaManager::get("warm_analog").unwrap();
    let f = test_stem_features();
    assert_eq!(SemanticZoneResolver::auto_carve(&p, &f),
               SemanticZoneResolver::auto_carve(&p, &f));
}
#[test]
fn zones_serializable() {
    let adj = ZoneAdjustments {
        bands: vec![ZoneAdjustment{center_hz:3000.0,gain_db:1.5,q:0.7}]
    };
    assert_eq!(adj, serde_json::from_str(
        &serde_json::to_string(&adj).unwrap()).unwrap());
}
```

---

## 12. Error Handling

| Condition | Behavior |
|-----------|----------|
| Empty / all inactive zones | `ZoneAdjustments::empty()` |
| Zone gain out of bounds | Clamped |
| Schema validation | Deferred to S-009 |

---

## 13. Implementation Path

```
aether/semantic/
├── mod.rs      ← pub use resolver::SemanticZoneResolver
├── zone.rs     ← SemanticZone, ZoneAdjustment, ZoneAdjustments, constants
├── resolver.rs ← resolve(), find_components(), resolve_component()
├── builder.rs  ← build_zones(), zone_gain_from_priority()
└── presets.rs  ← built-in zone type definitions
```

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-007_semantic_zones.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*Same zones + same priorities → same carve. Always.*
*Creative element lives in Chaos. Not here.*
