// aether/semantic/resolver.rs — SemanticZoneResolver
// Authority: spec/locked/S-007_semantic_zones.md v1.0
// Union-find clustering — one band per connected component.
// Priority-weighted average across entire component.
// BTreeMap — no HashSet (deterministic order guaranteed).

use std::collections::BTreeMap;
use lineos_types::analysis::StemFeatures;
use lineos_types::pre_analysis::PreAnalysisData;
use crate::personas::config::PersonaConfig;
use super::zone::*;

pub struct SemanticZoneResolver;

impl SemanticZoneResolver {
    /// Build default zone set from persona priorities and stem features.
    pub fn build_zones(persona:      &PersonaConfig,
                       features:     &StemFeatures,
                       pre_analysis: Option<&PreAnalysisData>,
    ) -> Vec<SemanticZone> {
        let mut zones = vec![];

        zones.push(SemanticZone {
            id: "dialogue".into(), center_hz: 3000.0, bandwidth_hz: 3000.0,
            gain_db: zone_gain_from_priority(
                persona.zone_priorities.dialogue, 0.0, 2.0),
            q: 0.7, priority: persona.zone_priorities.dialogue,
            active: true,
        });
        zones.push(SemanticZone {
            id: "bass".into(), center_hz: 120.0, bandwidth_hz: 190.0,
            gain_db: zone_gain_from_priority(
                persona.zone_priorities.bass, 0.0, 2.5),
            q: 0.5, priority: persona.zone_priorities.bass,
            active: true,
        });
        zones.push(SemanticZone {
            id: "air".into(), center_hz: 14000.0, bandwidth_hz: 10000.0,
            gain_db: zone_gain_from_priority(
                persona.zone_priorities.air, 0.0, 2.0),
            q: 0.4, priority: persona.zone_priorities.air,
            active: true,
        });

        // Zone flags — from PreAnalysis if available, fallback to stem features
        let cymbal_harsh = pre_analysis
            .map(|pa| pa.zone_flags.zone_cymbal_harsh)
            .unwrap_or(features.harmonics.spectral_crest_factor
                > CYMBAL_HARSH_CREST_THRESHOLD);

        let sub_rumble = pre_analysis
            .map(|pa| pa.zone_flags.zone_sub_rumble)
            .unwrap_or(features.mix.stem_energy_ratios[0]
                > SUB_RUMBLE_ENERGY_THRESHOLD);

        if cymbal_harsh {
            zones.push(SemanticZone {
                id: "cymbal_harsh".into(), center_hz: 9000.0,
                bandwidth_hz: 5000.0, gain_db: -1.5,
                q: 0.8, priority: 4, active: true,
            });
        }

        if sub_rumble {
            zones.push(SemanticZone {
                id: "sub_rumble".into(), center_hz: 40.0,
                bandwidth_hz: 40.0, gain_db: -1.0,
                q: 1.0, priority: 5, active: true,
            });
        }

        zones
    }

    /// Resolve zones into conflict-free EQ bands.
    /// One band per connected component of overlapping zones.
    /// Output sorted by center_hz (deterministic).
    pub fn resolve(zones: &[SemanticZone]) -> ZoneAdjustments {
        if zones.iter().all(|z| !z.active) {
            return ZoneAdjustments::empty();
        }
        let components = Self::find_components(zones);
        let mut bands: Vec<ZoneAdjustment> = components.iter()
            .map(|indices| Self::resolve_component(zones, indices))
            .collect();
        bands.sort_by(|a, b| a.center_hz.total_cmp(&b.center_hz));
        ZoneAdjustments { bands }
    }

    /// Full pipeline: build zones + resolve.
    pub fn auto_carve(persona:      &PersonaConfig,
                      features:     &StemFeatures,
                      pre_analysis: Option<&PreAnalysisData>,
    ) -> ZoneAdjustments {
        Self::resolve(&Self::build_zones(persona, features, pre_analysis))
    }

    /// Union-find: group overlapping active zones into components.
    /// Uses BTreeMap — deterministic key ordering guaranteed.
    pub fn find_components(zones: &[SemanticZone]) -> Vec<Vec<usize>> {
        let n = zones.len();
        let mut parent: Vec<usize> = (0..n).collect();

        fn find(parent: &mut Vec<usize>, i: usize) -> usize {
            if parent[i] != i {
                parent[i] = find(parent, parent[i]);
            }
            parent[i]
        }
        fn union(parent: &mut Vec<usize>, i: usize, j: usize) {
            let ri = find(parent, i);
            let rj = find(parent, j);
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

        let mut components: BTreeMap<usize, Vec<usize>> =
            BTreeMap::new();
        for (i, zone) in zones.iter().enumerate().take(n) {
            if zone.active {
                let root = find(&mut parent, i);
                components.entry(root).or_default().push(i);
            }
        }

        let mut result: Vec<Vec<usize>> = components
            .into_values()
            .map(|mut v| { v.sort_unstable(); v })
            .collect();
        result.sort_by_key(|c| c[0]);
        result
    }

    /// Priority-weighted average across all zones in component.
    fn resolve_component(zones: &[SemanticZone],
                         indices: &[usize]) -> ZoneAdjustment {
        debug_assert!(!indices.is_empty());

        if indices.len() == 1 {
            let z = &zones[indices[0]];
            return ZoneAdjustment {
                center_hz: z.center_hz,
                gain_db:   z.gain_db
                    .clamp(ZONE_GAIN_MIN_DB, ZONE_GAIN_MAX_DB),
                q:         z.q,
            };
        }

        let total_w: f32 = indices.iter()
            .map(|&i| zones[i].priority as f32)
            .sum();

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
}

fn zone_gain_from_priority(priority: u8,
                            min_gain: f32,
                            max_gain: f32) -> f32 {
    let t = (priority as f32 - 1.0) / 9.0;
    min_gain + t * (max_gain - min_gain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personas::manager::PersonaManager;

    fn test_stem_features() -> StemFeatures {
        use lineos_types::analysis::{StemMetrics, MixMetrics};
        StemFeatures {
            bass:      StemMetrics::default(),
            harmonics: StemMetrics::default(),
            voice:     StemMetrics::default(),
            drums:     StemMetrics::default(),
            ambience:  StemMetrics::default(),
            mix:       MixMetrics::default(),
        }
    }

    fn make_zone(id: &str, center_hz: f32, bandwidth_hz: f32,
                 gain_db: f32, q: f32, priority: u8) -> SemanticZone {
        SemanticZone {
            id: id.into(), center_hz, bandwidth_hz,
            gain_db, q, priority, active: true,
        }
    }

    #[test]
    fn zones_resolve_deterministic() {
        let p = PersonaManager::load().default_persona().clone();
        let f = test_stem_features();
        assert_eq!(
            SemanticZoneResolver::auto_carve(&p, &f, None),
            SemanticZoneResolver::auto_carve(&p, &f, None)
        );
    }

    #[test]
    fn zones_no_collision_passthrough() {
        let zones = vec![
            make_zone("bass",  120.0,   100.0, 1.5, 0.5, 7),
            make_zone("air",   14000.0, 8000.0, 1.0, 0.4, 8),
        ];
        assert_eq!(SemanticZoneResolver::resolve(&zones).bands.len(), 2);
    }

    #[test]
    fn zones_two_overlapping_one_band() {
        let zones = vec![
            make_zone("dialogue", 3000.0, 3000.0,  2.0, 0.7, 6),
            make_zone("traffic",  3000.0, 2000.0, -3.0, 0.8, 3),
        ];
        assert_eq!(SemanticZoneResolver::resolve(&zones).bands.len(), 1);
    }

    #[test]
    fn zones_three_overlapping_one_band() {
        let zones = vec![
            make_zone("a", 3000.0, 3000.0,  2.0, 0.7, 6),
            make_zone("b", 3000.0, 2000.0, -3.0, 0.8, 5),
            make_zone("c", 2500.0, 2000.0,  1.0, 0.6, 4),
        ];
        let r = SemanticZoneResolver::resolve(&zones);
        assert_eq!(r.bands.len(), 1,
            "Three overlapping zones must resolve to ONE band");
    }

    #[test]
    fn zones_three_weighted_average() {
        let zones = vec![
            make_zone("a", 3000.0, 3000.0,  2.0, 0.7, 6),
            make_zone("b", 3000.0, 2000.0, -3.0, 0.8, 5),
            make_zone("c", 2500.0, 2000.0,  1.0, 0.6, 4),
        ];
        let expected = (6.0*2.0 + 5.0*(-3.0) + 4.0*1.0) / 15.0;
        let r = SemanticZoneResolver::resolve(&zones);
        assert!((r.bands[0].gain_db - expected).abs() < 1e-4,
            "gain: {} vs expected {}", r.bands[0].gain_db, expected);
    }

    #[test]
    fn zones_output_sorted_by_center_hz() {
        let zones = vec![
            make_zone("air",  14000.0, 8000.0, 1.0, 0.4, 8),
            make_zone("bass",   120.0,  100.0, 1.5, 0.5, 7),
        ];
        let r = SemanticZoneResolver::resolve(&zones);
        assert!(r.bands[0].center_hz < r.bands[1].center_hz);
    }

    #[test]
    fn zones_gain_bounds_respected() {
        let zones = vec![make_zone("a", 1000.0, 500.0, 99.0, 1.0, 5)];
        let r = SemanticZoneResolver::resolve(&zones);
        assert!(r.bands[0].gain_db <= ZONE_GAIN_MAX_DB);
        assert!(r.bands[0].gain_db >= ZONE_GAIN_MIN_DB);
    }

    #[test]
    fn zones_inactive_excluded() {
        let mut zones = vec![
            make_zone("a", 3000.0, 2000.0, 2.0, 0.7, 6),
            make_zone("b",  120.0,  100.0, 1.5, 0.5, 7),
        ];
        zones[0].active = false;
        let r = SemanticZoneResolver::resolve(&zones);
        assert_eq!(r.bands.len(), 1);
        assert!((r.bands[0].center_hz - 120.0).abs() < 1e-4);
    }

    #[test]
    fn zones_serializable() {
        let adj = ZoneAdjustments {
            bands: vec![ZoneAdjustment {
                center_hz:3000.0, gain_db:1.5, q:0.7
            }]
        };
        let adj2: ZoneAdjustments = serde_json::from_str(
            &serde_json::to_string(&adj).unwrap()).unwrap();
        assert_eq!(adj, adj2);
    }
}
