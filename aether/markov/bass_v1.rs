use lineos_types::analysis::StemMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BassState {
    Silent = 0,
    Sustained = 1,
    Walking = 2,
    Punchy = 3,
    Rumble = 4,
}

impl BassState {
    pub fn to_str(&self) -> &'static str {
        match self {
            BassState::Silent => "silent",
            BassState::Sustained => "sustained",
            BassState::Walking => "walking",
            BassState::Punchy => "punchy",
            BassState::Rumble => "rumble",
        }
    }
}

pub const TRANSITION_MATRIX_BASS_V1: [[f32; 5]; 5] = [
    [0.80, 0.10, 0.05, 0.05, 0.00],
    [0.10, 0.70, 0.10, 0.05, 0.05],
    [0.05, 0.10, 0.60, 0.15, 0.10],
    [0.05, 0.05, 0.20, 0.50, 0.20],
    [0.10, 0.20, 0.20, 0.10, 0.40],
];

pub struct BassMarkovStateClassifier;

impl BassMarkovStateClassifier {
    pub fn classify_bass(m: &StemMetrics) -> BassState {
        if m.rms_db < -60.0 {
            return BassState::Silent;
        }
        if m.rms_db < -40.0 && m.crest_factor_db < 6.0 {
            return BassState::Rumble;
        }
        if m.rms_db >= -30.0 && m.crest_factor_db > 12.0 {
            return BassState::Punchy;
        }
        if m.rms_db >= -40.0 && m.crest_factor_db > 6.0 {
            return BassState::Walking;
        }
        BassState::Sustained
    }

    pub fn predict_next(state: BassState) -> BassState {
        let row = TRANSITION_MATRIX_BASS_V1[state as usize];
        let next_idx = row
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        match next_idx {
            0 => BassState::Silent,
            1 => BassState::Sustained,
            2 => BassState::Walking,
            3 => BassState::Punchy,
            _ => BassState::Rumble,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock(rms: f32, crest: f32) -> StemMetrics {
        StemMetrics {
            rms_db: rms,
            crest_factor_db: crest,
            ..StemMetrics::default()
        }
    }

    #[test]
    fn classify_silent() {
        assert_eq!(
            BassMarkovStateClassifier::classify_bass(&mock(-65.0, 5.0)),
            BassState::Silent
        );
    }

    #[test]
    fn predict_next_from_punchy_is_punchy() {
        assert_eq!(
            BassMarkovStateClassifier::predict_next(BassState::Punchy),
            BassState::Punchy
        );
    }

    #[test]
    fn transition_matrix_rows_sum_to_one() {
        for row in &TRANSITION_MATRIX_BASS_V1 {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6, "Row sum != 1.0: {}", sum);
        }
    }
}
