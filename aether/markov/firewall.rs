use super::predictive::{InstrumentDelta, MarkovDelta};

pub struct InstrumentBounds {
    pub level_max_db: f32,
    pub eq_max_db: f32,
    pub attack_max_ms: f32,
}

pub const DRUMS_BOUNDS: InstrumentBounds = InstrumentBounds {
    level_max_db: 3.0,
    eq_max_db: 2.0,
    attack_max_ms: 10.0,
};
pub const BASS_BOUNDS: InstrumentBounds = InstrumentBounds {
    level_max_db: 2.0,
    eq_max_db: 1.5,
    attack_max_ms: 5.0,
};
pub const HARMONICS_AMBIENCE_BOUNDS: InstrumentBounds = InstrumentBounds {
    level_max_db: 4.0,
    eq_max_db: 3.0,
    attack_max_ms: 15.0,
};

// INV-AB-7: last layer before DspConfig
// INV-AB-18: Voice > Drums > Bass > Harmonics > Ambience
pub struct IntegrationFirewall;

impl IntegrationFirewall {
    pub fn clamp_voice_delta(delta: MarkovDelta) -> MarkovDelta {
        MarkovDelta {
            comp_threshold_db: delta.comp_threshold_db.clamp(-3.0, 3.0),
            comp_attack_ms: delta.comp_attack_ms.clamp(-10.0, 10.0),
            comp_release_ms: delta.comp_release_ms.clamp(-20.0, 20.0),
            high_shelf_db: delta.high_shelf_db.clamp(-1.0, 1.0),
            low_shelf_db: delta.low_shelf_db.clamp(-1.0, 1.0),
            transient_risk: delta.transient_risk.clamp(0.0, 1.0),
            gap_risk: delta.gap_risk.clamp(0.0, 1.0),
        }
    }

    pub fn clamp_instrument_delta(
        delta: InstrumentDelta,
        bounds: &InstrumentBounds,
    ) -> InstrumentDelta {
        InstrumentDelta {
            comp_attack_ms: delta
                .comp_attack_ms
                .clamp(-bounds.attack_max_ms, bounds.attack_max_ms),
            eq_presence_db: delta
                .eq_presence_db
                .clamp(-bounds.eq_max_db, bounds.eq_max_db),
            comp_threshold_db: delta
                .comp_threshold_db
                .clamp(-bounds.level_max_db, bounds.level_max_db),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firewall_clamps_over_bounds() {
        let mut d = MarkovDelta::zero();
        d.comp_attack_ms = 15.0; // max is 10.0
        let clamped = IntegrationFirewall::clamp_voice_delta(d);
        assert_eq!(clamped.comp_attack_ms, 10.0);
    }

    #[test]
    fn firewall_clamps_under_bounds() {
        let mut d = MarkovDelta::zero();
        d.comp_threshold_db = -5.0; // min is -3.0
        let clamped = IntegrationFirewall::clamp_voice_delta(d);
        assert_eq!(clamped.comp_threshold_db, -3.0);
    }

    #[test]
    fn firewall_passes_valid_delta() {
        let mut d = MarkovDelta::zero();
        d.comp_attack_ms = 5.0;
        let clamped = IntegrationFirewall::clamp_voice_delta(d);
        assert_eq!(clamped.comp_attack_ms, 5.0);
    }
}
