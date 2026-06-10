// aether/chaos/seed.rs — Compound seed derivation
// Authority: spec/locked/S-006_chaos_engine.md v1.0
// SHA-256(project_id:track_id:persona_id)[0..8] as u64

use sha2::{Digest, Sha256};

/// Build deterministic compound seed.
/// Same project + track + persona → same chaos sequence.
/// Different persona → different sequence.
pub fn build_seed(project_id: &str, track_id: &str, persona_id: &str) -> u64 {
    let compound = format!("{}:{}:{}", project_id, track_id, persona_id);
    let hash = Sha256::digest(compound.as_bytes());
    u64::from_be_bytes(hash[0..8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_same_inputs_same_output() {
        assert_eq!(
            build_seed("p1", "t1", "warm_analog"),
            build_seed("p1", "t1", "warm_analog")
        );
    }

    #[test]
    fn seed_different_persona_different_seed() {
        assert_ne!(
            build_seed("p1", "t1", "warm_analog"),
            build_seed("p1", "t1", "clean_punch")
        );
    }
}
