// aether/mapping/mapper.rs — MacroMicroMapper
// Authority: spec/locked/S-005_macro_micro_mapping.md v1.0
// Pure function — same inputs → same MicroDelta, always.

use super::curves::apply_curve;
use super::types::*;
use crate::personas::config::{MacroControls, PersonaConfig};

pub struct MacroMicroMapper;

impl MacroMicroMapper {
    /// Map macro controls to DSP parameter deltas.
    /// All outputs clamped to constitutional bounds.
    /// StereoDelta.width is always 0.0 — set by S-006.
    pub fn map(persona: &PersonaConfig, macros: &MacroControls) -> MicroDelta {
        let w = apply_curve(macros.tone, &persona.macros.tone.curve);
        let p = apply_curve(macros.dynamics, &persona.macros.dynamics.curve);

        // ── EQ ────────────────────────────────────────────────
        let low_shelf_gain = (w * 3.0).clamp(EQ_GAIN_DELTA_MIN_DB, EQ_GAIN_DELTA_MAX_DB);
        let low_shelf_freq = (w * (-200.0)).clamp(EQ_FREQ_DELTA_MIN_HZ, EQ_FREQ_DELTA_MAX_HZ);

        let high_shelf_gain = 0.0_f32;
        let high_shelf_freq = 0.0_f32;

        // ── Dynamics ──────────────────────────────────────────
        let comp_threshold =
            (p * (-8.0)).clamp(COMP_THRESHOLD_DELTA_MIN_DB, COMP_THRESHOLD_DELTA_MAX_DB);
        let comp_ratio = (p * 2.0).clamp(COMP_RATIO_DELTA_MIN, COMP_RATIO_DELTA_MAX);
        let comp_attack = (p * (-15.0)).clamp(COMP_ATTACK_DELTA_MIN_MS, COMP_ATTACK_DELTA_MAX_MS);
        let comp_release = (p * -20.0).clamp(COMP_RELEASE_DELTA_MIN_MS, COMP_RELEASE_DELTA_MAX_MS);

        // ── Saturation ────────────────────────────────────────
        let sat_drive = (w * 0.3).clamp(SAT_DRIVE_DELTA_MIN, SAT_DRIVE_DELTA_MAX);
        let sat_mix = (w * 0.2).clamp(SAT_MIX_DELTA_MIN, SAT_MIX_DELTA_MAX);

        MicroDelta {
            eq: EqDelta {
                low_shelf_gain_db: low_shelf_gain,
                low_shelf_freq_hz: low_shelf_freq,
                high_shelf_gain_db: high_shelf_gain,
                high_shelf_freq_hz: high_shelf_freq,
            },
            dynamics: DynamicsDelta {
                comp_threshold_db: comp_threshold,
                comp_ratio,
                comp_attack_ms: comp_attack,
                comp_release_ms: comp_release,
            },
            sat: SaturationDelta {
                drive: sat_drive,
                mix: sat_mix,
            },
            // Stereo width reserved for S-006 (Chaos Engine)
            stereo: StereoDelta { width: 0.0 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personas::manager::PersonaManager;

    fn default_persona() -> PersonaConfig {
        PersonaManager::load().default_persona().clone()
    }

    #[test]
    fn mapping_deterministic() {
        let p = default_persona();
        let m = MacroControls {
            tone: 0.5,
            dynamics: 0.5,
        };
        assert_eq!(MacroMicroMapper::map(&p, &m), MacroMicroMapper::map(&p, &m));
    }

    #[test]
    fn mapping_zero_macros_near_zero_delta() {
        let p = default_persona();
        let m = MacroControls {
            tone: 0.0,
            dynamics: 0.0,
        };
        let d = MacroMicroMapper::map(&p, &m);
        assert!(d.eq.low_shelf_gain_db.abs() < 1e-5);
        assert!(d.dynamics.comp_threshold_db.abs() < 1e-5);
        assert!(d.sat.drive.abs() < 1e-5);
    }

    #[test]
    fn mapping_tone_increases_low_shelf() {
        let p = default_persona();
        let lo = MacroMicroMapper::map(
            &p,
            &MacroControls {
                tone: 0.1,
                dynamics: 0.5,
            },
        );
        let hi = MacroMicroMapper::map(
            &p,
            &MacroControls {
                tone: 0.9,
                dynamics: 0.5,
            },
        );
        assert!(hi.eq.low_shelf_gain_db > lo.eq.low_shelf_gain_db);
    }

    #[test]
    fn mapping_dynamics_tightens_compressor() {
        let p = default_persona();
        let lo = MacroMicroMapper::map(
            &p,
            &MacroControls {
                tone: 0.5,
                dynamics: 0.1,
            },
        );
        let hi = MacroMicroMapper::map(
            &p,
            &MacroControls {
                tone: 0.5,
                dynamics: 0.9,
            },
        );
        assert!(hi.dynamics.comp_threshold_db < lo.dynamics.comp_threshold_db);
        assert!(hi.dynamics.comp_ratio > lo.dynamics.comp_ratio);
    }

    #[test]
    fn mapping_stereo_width_always_zero() {
        let p = default_persona();
        for v in [0.0_f32, 0.5, 1.0] {
            let m = MacroControls {
                tone: v,
                dynamics: v,
            };
            assert_eq!(MacroMicroMapper::map(&p, &m).stereo.width, 0.0);
        }
    }

    #[test]
    fn mapping_bounds_respected() {
        let p = default_persona();
        let m = MacroControls {
            tone: 1.0,
            dynamics: 1.0,
        };
        let d = MacroMicroMapper::map(&p, &m);
        assert!(d.eq.low_shelf_gain_db >= EQ_GAIN_DELTA_MIN_DB);
        assert!(d.eq.low_shelf_gain_db <= EQ_GAIN_DELTA_MAX_DB);
        assert!(d.dynamics.comp_ratio >= COMP_RATIO_DELTA_MIN);
        assert!(d.dynamics.comp_ratio <= COMP_RATIO_DELTA_MAX);
        assert!(d.sat.drive >= SAT_DRIVE_DELTA_MIN);
        assert!(d.sat.drive <= SAT_DRIVE_DELTA_MAX);
    }

    #[test]
    fn mapping_serializable() {
        let p = default_persona();
        let m = MacroControls::default();
        let d = MacroMicroMapper::map(&p, &m);
        let json = serde_json::to_string(&d).unwrap();
        let d2: MicroDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(d, d2);
    }

    #[test]
    fn tone_above_zero_produces_nonzero_sat_mix() {
        use crate::personas::manager::PersonaManager;
        let persona = PersonaManager::load().default_persona().clone();
        let controls = MacroControls {
            tone: 0.5,
            dynamics: 0.0,
        };
        let delta = MacroMicroMapper::map(&persona, &controls);
        assert!(
            delta.sat.mix > 0.0,
            "tone=0.5 must produce sat_mix > 0.0, got {}",
            delta.sat.mix
        );
    }
}
