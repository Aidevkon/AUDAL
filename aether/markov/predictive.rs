use super::voice_v1::VoiceState;
use crate::markov::drums_v1::DrumsState;
use crate::markov::bass_v1::BassState;
use crate::markov::harmonics_v1::HarmonicsState;
use crate::markov::ambience_v1::AmbienceState;
use crate::simulation::SimulationDelta;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PredictiveDelta {
    pub pre_gain_db: f32,
    pub transient_pre_trigger: f32,
    pub density_bias: f32,
    pub ducking_hint: f32,
}

impl PredictiveDelta {
    pub fn zero() -> Self { Self::default() }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InstrumentDelta {
    pub comp_attack_ms: f32,
    pub eq_presence_db: f32,
    pub comp_threshold_db: f32,
}

impl InstrumentDelta {
    pub fn zero() -> Self { Self::default() }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InstrumentDeltas {
    pub drums: InstrumentDelta,
    pub bass: InstrumentDelta,
    pub harmonics: InstrumentDelta,
    pub ambience: InstrumentDelta,
}

#[derive(Clone)]
pub struct MarkovDelta {
    pub comp_threshold_db: f32,  // [-3.0, +3.0]
    pub comp_attack_ms:    f32,  // [-10.0, +10.0]
    pub comp_release_ms:   f32,  // [-20.0, +20.0]
    pub high_shelf_db:     f32,  // [-1.0, +1.0]
    pub low_shelf_db:      f32,  // [-1.0, +1.0]
    pub transient_risk:    f32,  // [0.0, 1.0]
    pub gap_risk:          f32,  // [0.0, 1.0]
}

impl MarkovDelta {
    pub fn zero() -> Self {
        Self {
            comp_threshold_db: 0.0,
            comp_attack_ms: 0.0,
            comp_release_ms: 0.0,
            high_shelf_db: 0.0,
            low_shelf_db: 0.0,
            transient_risk: 0.0,
            gap_risk: 0.0,
        }
    }
}

pub struct PredictiveController;

impl PredictiveController {
    /// Produce MarkovDelta from current + predicted VoiceState.
    /// INV-AB-1: deterministic. INV-AB-8: all values bounded.
    pub fn compute_voice_delta(
        _current: VoiceState,
        predicted: VoiceState,
    ) -> MarkovDelta {
        let mut delta = MarkovDelta::zero();
        match predicted {
            VoiceState::Silence => {
                // all 0.0
            }
            VoiceState::Breath => {
                delta.comp_threshold_db = 2.0;
                delta.comp_attack_ms = 5.0;
            }
            VoiceState::Consonant => {
                delta.comp_threshold_db = -2.0;
                delta.comp_attack_ms = -8.0;
            }
            VoiceState::Vowel => {
                delta.comp_attack_ms = 3.0;
                delta.high_shelf_db = 0.5;
            }
            VoiceState::Tail => {
                delta.comp_threshold_db = 3.0;
                delta.comp_attack_ms = 10.0;
                delta.high_shelf_db = -0.5;
            }
        }
        delta
    }

    /// SIM-P2: Evaluates the 6 Predictive Rules in priority order (Architecture §5).
    /// INV-AB-1: Deterministic evaluation.
    pub fn evaluate(markov: &MarkovDelta, sim: &SimulationDelta) -> PredictiveDelta {
        let mut delta = PredictiveDelta::zero();

        // Priority 1: Peak Protection
        if sim.predicted_peak > -1.0 {
            delta.pre_gain_db = -1.0 - sim.predicted_peak;
        }

        // Priority 2: Transient Protection
        delta.transient_pre_trigger = markov.transient_risk.clamp(0.0, 1.0);

        // Priority 3: Collapse Prevention
        if markov.gap_risk > 0.5 {
            delta.density_bias -= 0.1;
        }

        // Priority 4: GR Smoother
        if sim.predicted_gr > 6.0 {
            delta.density_bias -= 0.1;
        }

        // Priority 5: Gap Healing
        delta.ducking_hint = markov.gap_risk.clamp(0.0, 1.0);

        // Priority 6: Density Shaping
        delta.density_bias += (markov.transient_risk * 0.05) - (sim.crest_risk * 0.05);
        delta.density_bias = delta.density_bias.clamp(-0.3, 0.3); // Bounded

        delta
    }

    pub fn compute_drums_delta(predicted: DrumsState) -> InstrumentDelta {
        let mut delta = InstrumentDelta::zero();
        match predicted {
            DrumsState::Transient => { delta.comp_attack_ms = 5.0; delta.eq_presence_db = 1.0; },
            DrumsState::Decay => { delta.comp_attack_ms = -5.0; },
            _ => {}
        }
        delta
    }

    pub fn compute_bass_delta(predicted: BassState) -> InstrumentDelta {
        let mut delta = InstrumentDelta::zero();
        match predicted {
            BassState::Punchy => { delta.comp_attack_ms = 10.0; delta.eq_presence_db = 1.5; },
            BassState::Rumble => { delta.eq_presence_db = -1.0; },
            _ => {}
        }
        delta
    }

    pub fn compute_harmonics_delta(predicted: HarmonicsState) -> InstrumentDelta {
        let mut delta = InstrumentDelta::zero();
        match predicted {
            HarmonicsState::Bright => { delta.eq_presence_db = 2.0; },
            HarmonicsState::Warm => { delta.eq_presence_db = -1.0; },
            _ => {}
        }
        delta
    }

    pub fn compute_ambience_delta(predicted: AmbienceState) -> InstrumentDelta {
        let mut delta = InstrumentDelta::zero();
        match predicted {
            AmbienceState::Wash => { delta.eq_presence_db = -2.0; },
            AmbienceState::Lush => { delta.eq_presence_db = 1.0; },
            _ => {}
        }
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consonant_delta_fast_attack() {
        let delta = PredictiveController::compute_voice_delta(VoiceState::Vowel, VoiceState::Consonant);
        assert_eq!(delta.comp_attack_ms, -8.0);
    }

    #[test]
    fn vowel_delta_presence_boost() {
        let delta = PredictiveController::compute_voice_delta(VoiceState::Consonant, VoiceState::Vowel);
        assert_eq!(delta.high_shelf_db, 0.5);
    }

    #[test]
    fn silence_delta_is_zero() {
        let delta = PredictiveController::compute_voice_delta(VoiceState::Tail, VoiceState::Silence);
        assert_eq!(delta.comp_threshold_db, 0.0);
        assert_eq!(delta.comp_attack_ms, 0.0);
        assert_eq!(delta.comp_release_ms, 0.0);
        assert_eq!(delta.high_shelf_db, 0.0);
        assert_eq!(delta.low_shelf_db, 0.0);
        assert_eq!(delta.transient_risk, 0.0);
        assert_eq!(delta.gap_risk, 0.0);
    }

    #[test]
    fn all_deltas_within_bounds() {
        let states = [
            VoiceState::Silence,
            VoiceState::Breath,
            VoiceState::Consonant,
            VoiceState::Vowel,
            VoiceState::Tail,
        ];
        for &s in &states {
            let d = PredictiveController::compute_voice_delta(VoiceState::Silence, s);
            assert!(d.comp_threshold_db >= -3.0 && d.comp_threshold_db <= 3.0);
            assert!(d.comp_attack_ms >= -10.0 && d.comp_attack_ms <= 10.0);
            assert!(d.comp_release_ms >= -20.0 && d.comp_release_ms <= 20.0);
            assert!(d.high_shelf_db >= -1.0 && d.high_shelf_db <= 1.0);
            assert!(d.low_shelf_db >= -1.0 && d.low_shelf_db <= 1.0);
            assert!(d.transient_risk >= 0.0 && d.transient_risk <= 1.0);
            assert!(d.gap_risk >= 0.0 && d.gap_risk <= 1.0);
        }
    }

    #[test]
    fn deterministic() {
        let d1 = PredictiveController::compute_voice_delta(VoiceState::Silence, VoiceState::Breath);
        let d2 = PredictiveController::compute_voice_delta(VoiceState::Silence, VoiceState::Breath);
        assert_eq!(d1.comp_threshold_db, d2.comp_threshold_db);
        assert_eq!(d1.comp_attack_ms, d2.comp_attack_ms);
    }
}
