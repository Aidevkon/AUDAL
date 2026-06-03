use super::voice_v1::VoiceState;

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
