// integration/src/firewall.rs — IntegrationFirewall
// Authority: spec/locked/S-009_integration_firewall.md v1.0

use crate::config::*;
use crate::error::FirewallError;
use crate::proof_log::ProofLog;
use aether::chaos::ChaosDelta;
use aether::mapping::types::MicroDelta;
use aether::personas::config::{MacroControls, PersonaConfig};
use aether::semantic::ZoneAdjustments;
use serde::Serialize;

pub struct IntegrationFirewall;

impl IntegrationFirewall {
    /// Full pipeline: collect all Aether outputs → DspConfig.
    /// Validates, combines, clamps, logs.
    /// Returns Err on fatal error; Ok(DspConfig) on success.
    pub fn build(
        persona: &PersonaConfig,
        macros: &MacroControls,
        micro_delta: &MicroDelta,
        zone_adj: &ZoneAdjustments,
        chaos_delta: &ChaosDelta,
        chaos_seed: u64,
        proof_log: &mut ProofLog,
    ) -> Result<DspConfig, FirewallError> {
        // Step 1: Validate all Aether outputs against schemas
        // TODO v2.0: implement full schema validation with
        // include_str! embedded schemas.
        Self::validate_schema(persona, "persona.schema.json")?;
        Self::validate_schema(micro_delta, "dsp_config.schema.json")?;
        Self::validate_schema(zone_adj, "zone.schema.json")?;
        Self::validate_schema(chaos_delta, "chaos.schema.json")?;

        // Step 2: Validate macro controls [0.0, 1.0]
        for (field, val) in [("tone", macros.tone), ("dynamics", macros.dynamics)] {
            if !(0.0_f32..=1.0_f32).contains(&val) {
                return Err(FirewallError::MacroOutOfRange {
                    field: field.into(),
                    value: val,
                });
            }
        }

        // Step 3: Combine PersonaDspBase + MicroDelta + ChaosDelta
        let base = &persona.dsp_base;

        let raw_low_gain = base.low_shelf_gain_db + micro_delta.eq.low_shelf_gain_db;
        let raw_low_freq = base.low_shelf_freq_hz + micro_delta.eq.low_shelf_freq_hz;
        let raw_high_gain = base.high_shelf_gain_db
            + micro_delta.eq.high_shelf_gain_db
            + chaos_delta.air_shimmer_db;
        let raw_high_freq = base.high_shelf_freq_hz + micro_delta.eq.high_shelf_freq_hz;

        let raw_threshold = base.comp_threshold_db + micro_delta.dynamics.comp_threshold_db;
        let raw_ratio = base.comp_ratio + micro_delta.dynamics.comp_ratio;
        let raw_attack =
            base.comp_attack_ms + micro_delta.dynamics.comp_attack_ms + chaos_delta.comp_attack_mod;
        let raw_release = base.comp_release_ms
            + micro_delta.dynamics.comp_release_ms
            + chaos_delta.comp_release_mod;

        let raw_drive =
            base.saturation_drive + micro_delta.sat.drive + chaos_delta.sat_drive_mod_db;
        let raw_mix = base.saturation_mix + micro_delta.sat.mix;

        let raw_width = base.stereo_width + micro_delta.stereo.width + chaos_delta.stereo_width_mod;

        // Step 4: Constitutional clamp — log any violations
        let eq = DspEqConfig {
            low_shelf_gain_db: Self::clamp_log(
                raw_low_gain,
                CFW_EQ_GAIN_MIN_DB,
                CFW_EQ_GAIN_MAX_DB,
                "low_shelf_gain",
                proof_log,
            ),
            low_shelf_freq_hz: Self::clamp_log(
                raw_low_freq,
                CFW_EQ_FREQ_MIN_HZ,
                CFW_EQ_FREQ_MAX_HZ,
                "low_shelf_freq",
                proof_log,
            ),
            high_shelf_gain_db: Self::clamp_log(
                raw_high_gain,
                CFW_EQ_GAIN_MIN_DB,
                CFW_EQ_GAIN_MAX_DB,
                "high_shelf_gain",
                proof_log,
            ),
            high_shelf_freq_hz: Self::clamp_log(
                raw_high_freq,
                CFW_EQ_FREQ_MIN_HZ,
                CFW_EQ_FREQ_MAX_HZ,
                "high_shelf_freq",
                proof_log,
            ),
            // Zone bands preserved in center_hz-sorted order
            // (S-007 guarantees sort)
            zone_bands: zone_adj
                .bands
                .iter()
                .map(|b| ZoneBand {
                    center_hz: Self::clamp_log(
                        b.center_hz,
                        CFW_EQ_FREQ_MIN_HZ,
                        CFW_EQ_FREQ_MAX_HZ,
                        "zone_center",
                        proof_log,
                    ),
                    gain_db: Self::clamp_log(
                        b.gain_db,
                        CFW_EQ_GAIN_MIN_DB,
                        CFW_EQ_GAIN_MAX_DB,
                        "zone_gain",
                        proof_log,
                    ),
                    q: Self::clamp_log(b.q, CFW_EQ_Q_MIN, CFW_EQ_Q_MAX, "zone_q", proof_log),
                    source: b.source,
                })
                .collect(),
        };

        let dynamics = DspDynamicsConfig {
            comp_threshold_db: Self::clamp_log(
                raw_threshold,
                CFW_COMP_THRESHOLD_MIN_DB,
                CFW_COMP_THRESHOLD_MAX_DB,
                "comp_threshold",
                proof_log,
            ),
            comp_ratio: Self::clamp_log(
                raw_ratio,
                CFW_COMP_RATIO_MIN,
                CFW_COMP_RATIO_MAX,
                "comp_ratio",
                proof_log,
            ),
            comp_attack_ms: Self::clamp_log(
                raw_attack,
                CFW_COMP_ATTACK_MIN_MS,
                CFW_COMP_ATTACK_MAX_MS,
                "comp_attack",
                proof_log,
            ),
            comp_release_ms: Self::clamp_log(
                raw_release,
                CFW_COMP_RELEASE_MIN_MS,
                CFW_COMP_RELEASE_MAX_MS,
                "comp_release",
                proof_log,
            ),
        };

        let sat = DspSatConfig {
            drive: Self::clamp_log(
                raw_drive,
                CFW_SAT_DRIVE_MIN,
                CFW_SAT_DRIVE_MAX,
                "sat_drive",
                proof_log,
            ),
            mix: Self::clamp_log(
                raw_mix,
                CFW_SAT_MIX_MIN,
                CFW_SAT_MIX_MAX,
                "sat_mix",
                proof_log,
            ),
        };

        let stereo = DspStereoConfig {
            width: Self::clamp_log(
                raw_width,
                CFW_STEREO_WIDTH_MIN,
                CFW_STEREO_WIDTH_MAX,
                "stereo_width",
                proof_log,
            ),
        };

        // Step 5: Log final config to proof log (S-010)
        proof_log.record_dsp_config(&eq, &dynamics, &sat, &stereo);

        Ok(DspConfig {
            eq,
            dynamics,
            sat,
            stereo,
            ambience: None,
            persona_id: persona.id.clone(),
            chaos_seed,
            instrument_deltas: Default::default(),
        })
    }

    /// Validate schema — stub for v1.0.
    /// TODO v2.0: full validation with include_str! embedded schemas.
    pub fn validate_schema<T: Serialize>(
        _value: &T,
        _schema_name: &str,
    ) -> Result<(), FirewallError> {
        Ok(())
    }

    /// Clamp to constitutional bounds and log if clamping occurs.
    pub fn clamp_log(value: f32, min: f32, max: f32, field: &str, proof_log: &mut ProofLog) -> f32 {
        let clamped = value.clamp(min, max);
        if (clamped - value).abs() > 1e-6 {
            proof_log.record_clamp(FirewallError::FirewallClamp {
                field: field.to_string(),
                original: value,
                clamped,
            });
        }
        clamped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether::chaos::delta::ChaosDelta;
    use aether::mapping::mapper::MacroMicroMapper;
    use aether::personas::manager::PersonaManager;

    fn default_persona() -> PersonaConfig {
        PersonaManager::load().default_persona().clone()
    }

    fn default_macros() -> MacroControls {
        MacroControls::default()
    }

    fn extreme_micro_delta() -> MicroDelta {
        use aether::mapping::types::*;
        MicroDelta {
            eq: EqDelta {
                low_shelf_gain_db: 99.0,
                low_shelf_freq_hz: 99999.0,
                high_shelf_gain_db: 99.0,
                high_shelf_freq_hz: 99999.0,
            },
            dynamics: DynamicsDelta {
                comp_threshold_db: -999.0,
                comp_ratio: 99.0,
                comp_attack_ms: 99.0,
                comp_release_ms: 9999.0,
            },
            sat: SaturationDelta {
                drive: 99.0,
                mix: 99.0,
            },
            stereo: StereoDelta { width: 99.0 },
        }
    }

    fn extreme_chaos_delta() -> ChaosDelta {
        ChaosDelta {
            stereo_width_mod: 99.0,
            sat_drive_mod_db: 99.0,
            comp_release_mod: 99.0,
            comp_attack_mod: 99.0,
            air_shimmer_db: 99.0,
        }
    }

    fn test_firewall_inputs() -> (
        PersonaConfig,
        MacroControls,
        MicroDelta,
        ZoneAdjustments,
        ChaosDelta,
    ) {
        let persona = default_persona();
        let macros = default_macros();
        let micro = MacroMicroMapper::map(&persona, &macros);
        let zones = ZoneAdjustments::empty();
        let chaos = ChaosDelta::zero();
        (persona, macros, micro, zones, chaos)
    }

    #[test]
    fn firewall_build_deterministic() {
        let (persona, macros, micro, zones, chaos) = test_firewall_inputs();
        let (mut l1, mut l2) = (ProofLog::new(), ProofLog::new());
        assert_eq!(
            IntegrationFirewall::build(&persona, &macros, &micro, &zones, &chaos, 42, &mut l1)
                .unwrap(),
            IntegrationFirewall::build(&persona, &macros, &micro, &zones, &chaos, 42, &mut l2)
                .unwrap()
        );
    }

    #[test]
    fn firewall_constitutional_bounds_respected() {
        let persona = default_persona();
        let macros = MacroControls {
            tone: 1.0,
            dynamics: 1.0,
        };
        let mut log = ProofLog::new();
        let cfg = IntegrationFirewall::build(
            &persona,
            &macros,
            &extreme_micro_delta(),
            &ZoneAdjustments::empty(),
            &extreme_chaos_delta(),
            0,
            &mut log,
        )
        .unwrap();
        assert!(cfg.eq.low_shelf_gain_db >= CFW_EQ_GAIN_MIN_DB);
        assert!(cfg.eq.low_shelf_gain_db <= CFW_EQ_GAIN_MAX_DB);
        assert!(cfg.dynamics.comp_ratio >= CFW_COMP_RATIO_MIN);
        assert!(cfg.dynamics.comp_ratio <= CFW_COMP_RATIO_MAX);
        assert!(cfg.sat.drive >= CFW_SAT_DRIVE_MIN);
        assert!(cfg.sat.drive <= CFW_SAT_DRIVE_MAX);
        assert!(cfg.stereo.width >= CFW_STEREO_WIDTH_MIN);
        assert!(cfg.stereo.width <= CFW_STEREO_WIDTH_MAX);
    }

    #[test]
    fn firewall_clamp_logged_not_fatal() {
        let persona = default_persona();
        let mut log = ProofLog::new();
        let result = IntegrationFirewall::build(
            &persona,
            &default_macros(),
            &extreme_micro_delta(),
            &ZoneAdjustments::empty(),
            &ChaosDelta::zero(),
            0,
            &mut log,
        );
        assert!(result.is_ok());
        assert!(log.clamp_count() > 0);
    }

    #[test]
    fn firewall_invalid_macro_fatal() {
        let persona = default_persona();
        let macros = MacroControls {
            tone: 2.0,
            dynamics: 0.5,
        };
        let mut log = ProofLog::new();
        let result = IntegrationFirewall::build(
            &persona,
            &macros,
            &MicroDelta::default(),
            &ZoneAdjustments::empty(),
            &ChaosDelta::zero(),
            0,
            &mut log,
        );
        assert!(matches!(result, Err(FirewallError::MacroOutOfRange { .. })));
    }

    #[test]
    fn firewall_zone_bands_passed_through() {
        use aether::semantic::zone::ZoneAdjustment;
        let persona = default_persona();
        let zones = ZoneAdjustments {
            bands: vec![ZoneAdjustment {
                center_hz: 3000.0,
                gain_db: 1.5,
                q: 0.7,
                source: aether::semantic::zone::EqSource::Semantic,
            }],
        };
        let mut log = ProofLog::new();
        let cfg = IntegrationFirewall::build(
            &persona,
            &default_macros(),
            &MicroDelta::default(),
            &zones,
            &ChaosDelta::zero(),
            0,
            &mut log,
        )
        .unwrap();
        assert_eq!(cfg.eq.zone_bands.len(), 1);
        assert!((cfg.eq.zone_bands[0].center_hz - 3000.0).abs() < 1e-4);
    }

    #[test]
    fn firewall_persona_id_and_seed_in_config() {
        let (p, m, mi, z, c) = test_firewall_inputs();
        let mut log = ProofLog::new();
        let cfg = IntegrationFirewall::build(&p, &m, &mi, &z, &c, 42, &mut log).unwrap();
        assert_eq!(cfg.persona_id, "warm_analog");
        assert_eq!(cfg.chaos_seed, 42);
    }

    #[test]
    fn firewall_combination_order_correct() {
        let persona = default_persona();
        let base_gain = persona.dsp_base.low_shelf_gain_db;
        let mut micro = MicroDelta::default();
        micro.eq.low_shelf_gain_db = 1.0;
        let mut log = ProofLog::new();
        let cfg = IntegrationFirewall::build(
            &persona,
            &default_macros(),
            &micro,
            &ZoneAdjustments::empty(),
            &ChaosDelta::zero(),
            0,
            &mut log,
        )
        .unwrap();
        let expected = (base_gain + 1.0).clamp(CFW_EQ_GAIN_MIN_DB, CFW_EQ_GAIN_MAX_DB);
        assert!((cfg.eq.low_shelf_gain_db - expected).abs() < 1e-4);
    }

    #[test]
    fn firewall_config_serializable() {
        let (p, m, mi, z, c) = test_firewall_inputs();
        let mut log = ProofLog::new();
        let cfg = IntegrationFirewall::build(&p, &m, &mi, &z, &c, 0, &mut log).unwrap();
        let _: DspConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
    }
}
