use std::collections::HashMap;
use crate::analysis::role::Role;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Voice,
    Drums,
    Music,
}

#[derive(Debug, Clone)]
pub struct RoutedSource {
    pub id: usize,
    pub bus: Bus,
    pub confidence: f32,
    pub manual: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RoutingTable {
    sources: HashMap<usize, RoutedSource>,
}

impl RoutingTable {
    pub fn from_roles(roles: &[(usize, Role, f32)]) -> Self {
        let mut table = Self { sources: HashMap::new() };
        table.apply_roles(roles);
        table
    }

    pub fn apply_roles(&mut self, roles: &[(usize, Role, f32)]) {
        for &(id, role, confidence) in roles {
            if let Some(existing) = self.sources.get(&id) {
                if existing.manual {
                    continue;
                }
            }
            
            let bus = match role {
                Role::Voice => {
                    if confidence >= 0.75 { Bus::Voice } else { Bus::Music }
                }
                Role::Drums => {
                    if confidence >= 0.70 { Bus::Drums } else { Bus::Music }
                }
                Role::Music => Bus::Music,
            };

            self.sources.insert(id, RoutedSource {
                id,
                bus,
                confidence,
                manual: false,
            });
        }
    }

    pub fn assign(&mut self, id: usize, bus: Bus) {
        if let Some(source) = self.sources.get_mut(&id) {
            source.bus = bus;
            source.manual = true;
        } else {
            self.sources.insert(id, RoutedSource {
                id,
                bus,
                confidence: 1.0,
                manual: true,
            });
        }
    }

    pub fn sources_for(&self, bus: Bus) -> Vec<usize> {
        let mut list = Vec::new();
        for source in self.sources.values() {
            if source.bus == bus {
                list.push(source.id);
            }
        }
        list.sort_unstable();
        list
    }

    pub fn bus_of(&self, id: usize) -> Option<Bus> {
        self.sources.get(&id).map(|s| s.bus)
    }

    pub fn is_empty(&self, bus: Bus) -> bool {
        !self.sources.values().any(|s| s.bus == bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_routing() {
        // - three sources, one per role, land on their own buses
        let table = RoutingTable::from_roles(&[
            (1, Role::Voice, 0.8),
            (2, Role::Drums, 0.8),
            (3, Role::Music, 0.5),
        ]);
        assert_eq!(table.bus_of(1), Some(Bus::Voice));
        assert_eq!(table.bus_of(2), Some(Bus::Drums));
        assert_eq!(table.bus_of(3), Some(Bus::Music));

        // - a Voice role at confidence 0.60 falls to Music; at 0.80 it stays
        let table = RoutingTable::from_roles(&[
            (1, Role::Voice, 0.60),
            (2, Role::Voice, 0.80),
        ]);
        assert_eq!(table.bus_of(1), Some(Bus::Music));
        assert_eq!(table.bus_of(2), Some(Bus::Voice));

        // - a Drums role at 0.65 falls to Music; at 0.75 it stays
        let table = RoutingTable::from_roles(&[
            (1, Role::Drums, 0.65),
            (2, Role::Drums, 0.75),
        ]);
        assert_eq!(table.bus_of(1), Some(Bus::Music));
        assert_eq!(table.bus_of(2), Some(Bus::Drums));

        // - five sources classified Music all appear in sources_for(Music), in ascending id order
        let table = RoutingTable::from_roles(&[
            (10, Role::Music, 0.9),
            (3, Role::Music, 0.2),
            (7, Role::Music, 0.5),
            (1, Role::Music, 0.8),
            (5, Role::Music, 0.1),
        ]);
        assert_eq!(table.sources_for(Bus::Music), vec![1, 3, 5, 7, 10]);

        // - assign() moves a source, and a later from_roles-style reclassification does NOT move it back
        let mut table = RoutingTable::from_roles(&[
            (1, Role::Voice, 0.9),
        ]);
        assert_eq!(table.bus_of(1), Some(Bus::Voice));
        table.assign(1, Bus::Music);
        assert_eq!(table.bus_of(1), Some(Bus::Music));
        
        table.apply_roles(&[(1, Role::Voice, 0.99)]);
        assert_eq!(table.bus_of(1), Some(Bus::Music)); // Manual override persists

        // - is_empty(Drums) is true for a voice-only set
        let table = RoutingTable::from_roles(&[
            (1, Role::Voice, 0.9),
            (2, Role::Voice, 0.8),
        ]);
        assert!(table.is_empty(Bus::Drums));
        assert!(!table.is_empty(Bus::Voice));
    }
}
