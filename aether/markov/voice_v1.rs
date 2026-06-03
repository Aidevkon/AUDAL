use lineos_types::analysis::StemMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    Silence   = 0,
    Breath    = 1,
    Consonant = 2,
    Vowel     = 3,
    Tail      = 4,
}

/// M2: VoiceV1 hand-tuned transition matrix (INV-AB-2: compile-time const)
pub const TRANSITION_MATRIX_VOICE_V1: [[f32; 5]; 5] = [
    [0.70, 0.15, 0.05, 0.05, 0.05],
    [0.10, 0.20, 0.40, 0.20, 0.10],
    [0.05, 0.05, 0.15, 0.65, 0.10],
    [0.05, 0.05, 0.20, 0.55, 0.15],
    [0.30, 0.20, 0.10, 0.10, 0.30],
];

pub struct MarkovStateClassifier;

impl MarkovStateClassifier {
    /// M3: Classify voice state from StemMetrics.
    /// INV-AB-1: deterministic — same input → same state always.
    pub fn classify_voice(m: &StemMetrics) -> VoiceState {
        if m.rms_db < -60.0 {
            return VoiceState::Silence;
        }
        if m.rms_db < -30.0 && m.crest_factor_db < 8.0 {
            return VoiceState::Breath;
        }
        if m.rms_db >= -30.0 && m.crest_factor_db > 12.0 {
            return VoiceState::Consonant;
        }
        if m.rms_db >= -18.0 && m.crest_factor_db <= 10.0 {
            return VoiceState::Vowel;
        }
        VoiceState::Tail
    }

    /// Next state prediction: argmax of transition row (INV-AB-2: no sampling).
    pub fn predict_next(state: VoiceState) -> VoiceState {
        let row = TRANSITION_MATRIX_VOICE_V1[state as usize];
        let next_idx = row.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        match next_idx {
            0 => VoiceState::Silence,
            1 => VoiceState::Breath,
            2 => VoiceState::Consonant,
            3 => VoiceState::Vowel,
            _ => VoiceState::Tail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock(rms: f32, crest: f32) -> StemMetrics {
        StemMetrics { rms_db: rms, crest_factor_db: crest, ..StemMetrics::default() }
    }

    #[test]
    fn classify_silence() {
        assert_eq!(MarkovStateClassifier::classify_voice(&mock(-65.0, 5.0)), VoiceState::Silence);
    }
    #[test]
    fn classify_breath() {
        assert_eq!(MarkovStateClassifier::classify_voice(&mock(-45.0, 5.0)), VoiceState::Breath);
    }
    #[test]
    fn classify_consonant() {
        assert_eq!(MarkovStateClassifier::classify_voice(&mock(-20.0, 15.0)), VoiceState::Consonant);
    }
    #[test]
    fn classify_vowel() {
        assert_eq!(MarkovStateClassifier::classify_voice(&mock(-12.0, 6.0)), VoiceState::Vowel);
    }
    #[test]
    fn classify_tail() {
        assert_eq!(MarkovStateClassifier::classify_voice(&mock(-25.0, 8.0)), VoiceState::Tail);
    }
    #[test]
    fn predict_next_from_consonant_is_vowel() {
        // Consonant → Vowel has highest probability (0.65)
        assert_eq!(MarkovStateClassifier::predict_next(VoiceState::Consonant), VoiceState::Vowel);
    }
    #[test]
    fn transition_matrix_rows_sum_to_one() {
        for row in &TRANSITION_MATRIX_VOICE_V1 {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6, "Row sum != 1.0: {}", sum);
        }
    }
}
