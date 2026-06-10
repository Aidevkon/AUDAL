use super::predictive::MarkovDelta;

// INV-AB-14: max 30% deviation
// INV-AB-15: bypassable globally and per-stem
pub struct ChaosLayer {
    pub bypass: bool,
    pub seed:   u64,  // deterministic — from chaos_seed in DspConfig
}

impl ChaosLayer {
    pub fn modulate(&self, mut delta: MarkovDelta, mut seed: u64) -> MarkovDelta {
        if self.bypass { return delta; }
        
        // Deterministic micro-variation using seed
        // Max 30% deviation per field (INV-AB-14)
        // No rand:: — use seed-based hash function (LCG)
        let mut next_rand = || -> f32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let val = (seed >> 32) as u32;
            let f = val as f32 / u32::MAX as f32;
            (f * 2.0) - 1.0 // [-1.0, 1.0]
        };

        let max_dev = 0.30;

        delta.comp_threshold_db += delta.comp_threshold_db * max_dev * next_rand();
        delta.comp_attack_ms    += delta.comp_attack_ms * max_dev * next_rand();
        delta.comp_release_ms   += delta.comp_release_ms * max_dev * next_rand();
        delta.high_shelf_db     += delta.high_shelf_db * max_dev * next_rand();
        delta.low_shelf_db      += delta.low_shelf_db * max_dev * next_rand();
        
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaos_bypass_returns_unchanged() {
        let chaos = ChaosLayer { bypass: true, seed: 42 };
        let mut d = MarkovDelta::zero();
        d.comp_attack_ms = 10.0;
        let d2 = chaos.modulate(d, 42);
        assert_eq!(d2.comp_attack_ms, 10.0);
    }

    #[test]
    fn chaos_max_deviation_30_percent() {
        let chaos = ChaosLayer { bypass: false, seed: 42 };
        let mut d = MarkovDelta::zero();
        d.comp_attack_ms = 10.0;
        let d2 = chaos.modulate(d, 42);
        let diff = (d2.comp_attack_ms - 10.0).abs();
        assert!(diff <= 3.0001); // 30% of 10.0 is 3.0
    }

    #[test]
    fn chaos_deterministic_same_seed() {
        let chaos = ChaosLayer { bypass: false, seed: 123 };
        
        let mut d1 = MarkovDelta::zero();
        d1.comp_attack_ms = 10.0;
        let d1_mod = chaos.modulate(d1, 123);
        
        let mut d2 = MarkovDelta::zero();
        d2.comp_attack_ms = 10.0;
        let d2_mod = chaos.modulate(d2, 123);
        
        assert_eq!(d1_mod.comp_attack_ms, d2_mod.comp_attack_ms);
    }
}
