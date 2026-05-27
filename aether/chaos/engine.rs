// aether/chaos/engine.rs — ChaosEngine
// Authority: spec/locked/S-006_chaos_engine.md v1.0
// Logistic map: x[n+1] = r × x[n] × (1 - x[n]), r=3.9
// 100 warmup iterations — decorrelates state from seed bias.

use crate::personas::config::ChaosProfile;
use crate::mapping::types::{
    MicroDelta, EqDelta, DynamicsDelta, SaturationDelta, StereoDelta,
    EQ_GAIN_DELTA_MIN_DB, EQ_GAIN_DELTA_MAX_DB,
    COMP_ATTACK_DELTA_MIN_MS, COMP_ATTACK_DELTA_MAX_MS,
    COMP_RELEASE_DELTA_MIN_MS, COMP_RELEASE_DELTA_MAX_MS,
    SAT_DRIVE_DELTA_MIN, SAT_DRIVE_DELTA_MAX,
    STEREO_WIDTH_DELTA_MIN, STEREO_WIDTH_DELTA_MAX,
};
use super::delta::{ChaosDelta, CHAOS_R, CHAOS_WARMUP_ITERS,
                   CHAOS_WIDTH_MAX, CHAOS_DRIVE_MAX_DB,
                   CHAOS_RELEASE_MAX, CHAOS_ATTACK_MAX, CHAOS_SHIMMER_MAX};
use super::seed::build_seed;

pub struct ChaosEngine {
    state: f32,
}

#[inline]
fn logistic_next(x: f32) -> f32 {
    CHAOS_R * x * (1.0_f32 - x)
}

impl ChaosEngine {
    /// Initialise from compound seed.
    /// Runs CHAOS_WARMUP_ITERS to decorrelate state from seed bias.
    pub fn new(seed: u64) -> Self {
        let x0 = (seed as f32 / u64::MAX as f32)
            .clamp(0.001_f32, 0.999_f32);
        let mut x = x0;
        for _ in 0..CHAOS_WARMUP_ITERS {
            x = logistic_next(x);
        }
        Self { state: x }
    }

    /// Build compound seed from identifiers (delegates to seed.rs).
    pub fn build_seed(project_id: &str, track_id: &str,
                      persona_id: &str) -> u64 {
        build_seed(project_id, track_id, persona_id)
    }

    /// Generate next ChaosDelta.
    /// Advances logistic map state 5 times (one per target).
    /// Scaled by: chaos_intensity × ChaosProfile.X_depth × CHAOS_X_MAX.
    pub fn next_delta(&mut self, chaos_intensity: f32,
                      profile: &ChaosProfile) -> ChaosDelta {
        let ci = chaos_intensity.clamp(0.0_f32, 1.0_f32);
        if ci == 0.0 { return ChaosDelta::zero(); }

        // Step 1: stereo width
        self.state = logistic_next(self.state);
        let w = Self::modulate(0.0,
            CHAOS_WIDTH_MAX * profile.width_depth * ci,
            self.state, -CHAOS_WIDTH_MAX, CHAOS_WIDTH_MAX);

        // Step 2: saturation drive
        self.state = logistic_next(self.state);
        let d = Self::modulate(0.0,
            CHAOS_DRIVE_MAX_DB * profile.drive_depth * ci,
            self.state, -CHAOS_DRIVE_MAX_DB, CHAOS_DRIVE_MAX_DB);

        // Step 3: compressor release
        self.state = logistic_next(self.state);
        let r = Self::modulate(0.0,
            CHAOS_RELEASE_MAX * profile.release_depth * ci,
            self.state, -CHAOS_RELEASE_MAX, CHAOS_RELEASE_MAX);

        // Step 4: compressor attack
        self.state = logistic_next(self.state);
        let a = Self::modulate(0.0,
            CHAOS_ATTACK_MAX * profile.attack_depth * ci,
            self.state, -CHAOS_ATTACK_MAX, CHAOS_ATTACK_MAX);

        // Step 5: air shimmer
        self.state = logistic_next(self.state);
        let s = Self::modulate(0.0,
            CHAOS_SHIMMER_MAX * profile.air_depth * ci,
            self.state, -CHAOS_SHIMMER_MAX, CHAOS_SHIMMER_MAX);

        ChaosDelta {
            stereo_width_mod: w,
            sat_drive_mod_db: d,
            comp_release_mod: r,
            comp_attack_mod:  a,
            air_shimmer_db:   s,
        }
    }

    /// RFC-001 Appendix modulate() pattern:
    /// output = clamp(base + (chaos_val - 0.5) × 2.0 × depth, min, max)
    pub fn modulate(base: f32, depth: f32, chaos_val: f32,
                    min: f32, max: f32) -> f32 {
        let delta = (chaos_val - 0.5_f32) * 2.0_f32 * depth;
        (base + delta).clamp(min, max)
    }

    /// Apply ChaosDelta to MicroDelta.
    /// All results clamped to S-005 constitutional bounds.
    /// air_shimmer_db modulates high_shelf_gain_db ONLY (not freq).
    pub fn apply(base: &MicroDelta, delta: &ChaosDelta) -> MicroDelta {
        MicroDelta {
            eq: EqDelta {
                low_shelf_gain_db:  base.eq.low_shelf_gain_db,
                low_shelf_freq_hz:  base.eq.low_shelf_freq_hz,
                high_shelf_gain_db: (base.eq.high_shelf_gain_db
                                     + delta.air_shimmer_db)
                    .clamp(EQ_GAIN_DELTA_MIN_DB, EQ_GAIN_DELTA_MAX_DB),
                high_shelf_freq_hz: base.eq.high_shelf_freq_hz,
            },
            dynamics: DynamicsDelta {
                comp_threshold_db: base.dynamics.comp_threshold_db,
                comp_ratio:        base.dynamics.comp_ratio,
                comp_attack_ms:    (base.dynamics.comp_attack_ms
                                    + delta.comp_attack_mod)
                    .clamp(COMP_ATTACK_DELTA_MIN_MS, COMP_ATTACK_DELTA_MAX_MS),
                comp_release_ms:   (base.dynamics.comp_release_ms
                                    + delta.comp_release_mod)
                    .clamp(COMP_RELEASE_DELTA_MIN_MS, COMP_RELEASE_DELTA_MAX_MS),
            },
            sat: SaturationDelta {
                drive: (base.sat.drive + delta.sat_drive_mod_db)
                    .clamp(SAT_DRIVE_DELTA_MIN, SAT_DRIVE_DELTA_MAX),
                mix:   base.sat.mix,
            },
            stereo: StereoDelta {
                width: delta.stereo_width_mod
                    .clamp(STEREO_WIDTH_DELTA_MIN, STEREO_WIDTH_DELTA_MAX),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personas::config::ChaosProfile;
    use crate::mapping::mapper::MacroMicroMapper;
    use crate::mapping::types::EQ_GAIN_DELTA_MAX_DB;
    use crate::personas::manager::PersonaManager;

    fn test_profile() -> ChaosProfile {
        ChaosProfile {
            width_depth:0.5, drive_depth:0.5, release_depth:0.5,
            attack_depth:0.5, air_depth:0.5,
        }
    }

    fn full_profile() -> ChaosProfile {
        ChaosProfile {
            width_depth:1.0, drive_depth:1.0, release_depth:1.0,
            attack_depth:1.0, air_depth:1.0,
        }
    }

    fn test_base_delta() -> MicroDelta {
        let p = PersonaManager::load().default_persona().clone();
        let m = crate::personas::config::MacroControls::default();
        MacroMicroMapper::map(&p, &m)
    }

    fn test_base_delta_maxed() -> MicroDelta {
        let p = PersonaManager::load().default_persona().clone();
        let m = crate::personas::config::MacroControls {
            warmth:1.0, punch:1.0, forwardness:1.0, smoothness:1.0
        };
        MacroMicroMapper::map(&p, &m)
    }

    #[test]
    fn chaos_same_seed_same_sequence() {
        let p = test_profile();
        let mut e1 = ChaosEngine::new(42);
        let mut e2 = ChaosEngine::new(42);
        assert_eq!(e1.next_delta(0.5, &p), e2.next_delta(0.5, &p));
    }

    #[test]
    fn chaos_compound_seed_persona_differentiates() {
        let s1 = ChaosEngine::build_seed("p1", "t1", "warm_analog");
        let s2 = ChaosEngine::build_seed("p1", "t1", "clean_punch");
        assert_ne!(s1, s2);
    }

    #[test]
    fn chaos_compound_seed_reproducible() {
        assert_eq!(
            ChaosEngine::build_seed("p1", "t1", "warm_analog"),
            ChaosEngine::build_seed("p1", "t1", "warm_analog")
        );
    }

    #[test]
    fn chaos_zero_intensity_zero_delta() {
        let mut e = ChaosEngine::new(42);
        assert_eq!(e.next_delta(0.0, &full_profile()), ChaosDelta::zero());
    }

    #[test]
    fn chaos_zero_profile_zero_delta() {
        let mut e = ChaosEngine::new(42);
        let zero_p = ChaosProfile {
            width_depth:0.0, drive_depth:0.0, release_depth:0.0,
            attack_depth:0.0, air_depth:0.0,
        };
        assert_eq!(e.next_delta(1.0, &zero_p), ChaosDelta::zero());
    }

    #[test]
    fn chaos_bounds_at_full_intensity() {
        let mut e = ChaosEngine::new(12345);
        for _ in 0..1000 {
            let d = e.next_delta(1.0, &full_profile());
            assert!(d.stereo_width_mod.abs() <= CHAOS_WIDTH_MAX    + 1e-5);
            assert!(d.sat_drive_mod_db.abs() <= CHAOS_DRIVE_MAX_DB + 1e-5);
            assert!(d.comp_release_mod.abs() <= CHAOS_RELEASE_MAX  + 1e-5);
            assert!(d.comp_attack_mod.abs()  <= CHAOS_ATTACK_MAX   + 1e-5);
            assert!(d.air_shimmer_db.abs()   <= CHAOS_SHIMMER_MAX  + 1e-5);
        }
    }

    #[test]
    fn chaos_apply_zero_delta_passthrough() {
        let base   = test_base_delta();
        let result = ChaosEngine::apply(&base, &ChaosDelta::zero());
        assert_eq!(result, base);
    }

    #[test]
    fn chaos_apply_air_shimmer_gain_only() {
        let base        = test_base_delta();
        let freq_before = base.eq.high_shelf_freq_hz;
        let delta       = ChaosDelta { air_shimmer_db: 0.3,
                                        ..ChaosDelta::zero() };
        let result      = ChaosEngine::apply(&base, &delta);
        assert_eq!(result.eq.high_shelf_freq_hz, freq_before);
        assert!((result.eq.high_shelf_gain_db
                 - (base.eq.high_shelf_gain_db + 0.3)).abs() < 1e-4
                || result.eq.high_shelf_gain_db == EQ_GAIN_DELTA_MAX_DB);
    }

    #[test]
    fn chaos_apply_bounds_respected() {
        let base  = test_base_delta_maxed();
        let mut e = ChaosEngine::new(99999);
        for _ in 0..100 {
            let delta  = e.next_delta(1.0, &full_profile());
            let result = ChaosEngine::apply(&base, &delta);
            assert!(result.eq.high_shelf_gain_db >= EQ_GAIN_DELTA_MIN_DB);
            assert!(result.eq.high_shelf_gain_db <= EQ_GAIN_DELTA_MAX_DB);
            assert!(result.dynamics.comp_attack_ms  >= COMP_ATTACK_DELTA_MIN_MS);
            assert!(result.dynamics.comp_attack_ms  <= COMP_ATTACK_DELTA_MAX_MS);
            assert!(result.stereo.width >= STEREO_WIDTH_DELTA_MIN);
            assert!(result.stereo.width <= STEREO_WIDTH_DELTA_MAX);
        }
    }

    #[test]
    fn logistic_map_unit_interval() {
        let mut x = 0.5_f32;
        for _ in 0..10_000 {
            x = CHAOS_R * x * (1.0 - x);
            assert!(x > 0.0 && x < 1.0,
                "logistic map left unit interval: {}", x);
        }
    }

    #[test]
    fn modulate_symmetric_at_midpoint() {
        let result = ChaosEngine::modulate(0.3, 0.1, 0.5, -1.0, 1.0);
        assert!((result - 0.3).abs() < 1e-6);
    }

    #[test]
    fn chaos_serializable() {
        let d = ChaosDelta {
            stereo_width_mod:0.05, sat_drive_mod_db:0.3,
            comp_release_mod:5.0,  comp_attack_mod:1.0,
            air_shimmer_db:0.2,
        };
        let json = serde_json::to_string(&d).unwrap();
        let d2: ChaosDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(d, d2);
    }
}
