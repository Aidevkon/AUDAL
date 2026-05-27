// integration/src/overrides.rs — DspOverrides
// Authority: spec/locked/S-011a_multimodal_control.md v1.0
// DspOverrides lives in integration/ because apply() needs DspConfig.
// Placement here avoids aether → integration circular dependency.

use crate::config::*;

/// Direct DSP parameter overrides — applied AFTER Aether processing.
///
/// Overrides are applied after all Aether outputs (persona, macro,
/// chaos, zones) have been combined by S-009. They replace only the
/// specified fields; all other DSP parameters remain from Aether.
/// Chaos and zone adjustments are NOT disabled by overrides.
///
/// None = use Aether value. Some(v) = replace with v (clamped).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DspOverrides {
    pub low_shelf_gain_db:  Option<f32>,
    pub low_shelf_freq_hz:  Option<f32>,
    pub high_shelf_gain_db: Option<f32>,
    pub high_shelf_freq_hz: Option<f32>,
    pub comp_threshold_db:  Option<f32>,
    pub comp_ratio:         Option<f32>,
    pub comp_attack_ms:     Option<f32>,
    pub comp_release_ms:    Option<f32>,
    pub sat_drive:          Option<f32>,
    pub sat_mix:            Option<f32>,
    pub stereo_width:       Option<f32>,
}

impl DspOverrides {
    /// Apply overrides to a validated DspConfig from S-009.
    /// Some(v) → replace with v, clamped to constitutional bounds.
    /// None    → keep Aether value unchanged.
    pub fn apply(&self, config: &DspConfig) -> DspConfig {
        DspConfig {
            eq: DspEqConfig {
                low_shelf_gain_db:  self.low_shelf_gain_db
                    .map(|v| v.clamp(CFW_EQ_GAIN_MIN_DB,
                                     CFW_EQ_GAIN_MAX_DB))
                    .unwrap_or(config.eq.low_shelf_gain_db),
                low_shelf_freq_hz:  self.low_shelf_freq_hz
                    .map(|v| v.clamp(CFW_EQ_FREQ_MIN_HZ,
                                     CFW_EQ_FREQ_MAX_HZ))
                    .unwrap_or(config.eq.low_shelf_freq_hz),
                high_shelf_gain_db: self.high_shelf_gain_db
                    .map(|v| v.clamp(CFW_EQ_GAIN_MIN_DB,
                                     CFW_EQ_GAIN_MAX_DB))
                    .unwrap_or(config.eq.high_shelf_gain_db),
                high_shelf_freq_hz: self.high_shelf_freq_hz
                    .map(|v| v.clamp(CFW_EQ_FREQ_MIN_HZ,
                                     CFW_EQ_FREQ_MAX_HZ))
                    .unwrap_or(config.eq.high_shelf_freq_hz),
                zone_bands: config.eq.zone_bands.clone(),
            },
            dynamics: DspDynamicsConfig {
                comp_threshold_db: self.comp_threshold_db
                    .map(|v| v.clamp(CFW_COMP_THRESHOLD_MIN_DB,
                                     CFW_COMP_THRESHOLD_MAX_DB))
                    .unwrap_or(config.dynamics.comp_threshold_db),
                comp_ratio:        self.comp_ratio
                    .map(|v| v.clamp(CFW_COMP_RATIO_MIN,
                                     CFW_COMP_RATIO_MAX))
                    .unwrap_or(config.dynamics.comp_ratio),
                comp_attack_ms:    self.comp_attack_ms
                    .map(|v| v.clamp(CFW_COMP_ATTACK_MIN_MS,
                                     CFW_COMP_ATTACK_MAX_MS))
                    .unwrap_or(config.dynamics.comp_attack_ms),
                comp_release_ms:   self.comp_release_ms
                    .map(|v| v.clamp(CFW_COMP_RELEASE_MIN_MS,
                                     CFW_COMP_RELEASE_MAX_MS))
                    .unwrap_or(config.dynamics.comp_release_ms),
            },
            sat: DspSatConfig {
                drive: self.sat_drive
                    .map(|v| v.clamp(CFW_SAT_DRIVE_MIN,
                                     CFW_SAT_DRIVE_MAX))
                    .unwrap_or(config.sat.drive),
                mix:   self.sat_mix
                    .map(|v| v.clamp(CFW_SAT_MIX_MIN,
                                     CFW_SAT_MIX_MAX))
                    .unwrap_or(config.sat.mix),
            },
            stereo: DspStereoConfig {
                width: self.stereo_width
                    .map(|v| v.clamp(CFW_STEREO_WIDTH_MIN,
                                     CFW_STEREO_WIDTH_MAX))
                    .unwrap_or(config.stereo.width),
            },
            persona_id: config.persona_id.clone(),
            chaos_seed: config.chaos_seed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether::personas::manager::PersonaManager;
    use aether::mapping::mapper::MacroMicroMapper;
    use aether::personas::config::MacroControls;
    use aether::semantic::ZoneAdjustments;
    use aether::chaos::ChaosDelta;
    use crate::firewall::IntegrationFirewall;
    use crate::proof_log::ProofLog;

    fn test_dsp_config() -> DspConfig {
        let persona = PersonaManager::load().default_persona().clone();
        let macros  = MacroControls::default();
        let micro   = MacroMicroMapper::map(&persona, &macros);
        let mut log = ProofLog::new();
        IntegrationFirewall::build(
            &persona, &macros, &micro,
            &ZoneAdjustments::empty(),
            &ChaosDelta::zero(), 0, &mut log
        ).unwrap()
    }

    #[test]
    fn overrides_none_passthrough() {
        let cfg    = test_dsp_config();
        let result = DspOverrides::default().apply(&cfg);
        assert_eq!(result.eq.low_shelf_gain_db,
                   cfg.eq.low_shelf_gain_db);
        assert_eq!(result.dynamics.comp_ratio,
                   cfg.dynamics.comp_ratio);
    }

    #[test]
    fn overrides_some_applied_and_clamped() {
        let cfg = test_dsp_config();
        let ov  = DspOverrides {
            comp_ratio: Some(99.0),
            ..Default::default()
        };
        let result = ov.apply(&cfg);
        assert!(result.dynamics.comp_ratio <= CFW_COMP_RATIO_MAX);
    }

    #[test]
    fn overrides_preserves_zone_bands() {
        let cfg    = test_dsp_config();
        let result = DspOverrides::default().apply(&cfg);
        assert_eq!(result.eq.zone_bands.len(),
                   cfg.eq.zone_bands.len());
    }

    #[test]
    fn overrides_persona_id_preserved() {
        let cfg    = test_dsp_config();
        let result = DspOverrides::default().apply(&cfg);
        assert_eq!(result.persona_id, cfg.persona_id);
    }
}
