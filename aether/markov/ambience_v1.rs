use lineos_types::analysis::StemMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbienceState {
    Dry = 0,
    Subtle = 1,
    Present = 2,
    Lush = 3,
    Wash = 4,
}

impl AmbienceState {
    pub fn to_str(&self) -> &'static str {
        match self {
            AmbienceState::Dry => "dry",
            AmbienceState::Subtle => "subtle",
            AmbienceState::Present => "present",
            AmbienceState::Lush => "lush",
            AmbienceState::Wash => "wash",
        }
    }
}

pub const TRANSITION_MATRIX_AMBIENCE_V1: [[f32; 5]; 5] = [
    [0.60, 0.30, 0.10, 0.00, 0.00],
    [0.10, 0.60, 0.20, 0.05, 0.05],
    [0.05, 0.15, 0.60, 0.10, 0.10],
    [0.05, 0.10, 0.15, 0.50, 0.20],
    [0.10, 0.05, 0.15, 0.20, 0.50],
];

pub struct AmbienceMarkovStateClassifier;

impl AmbienceMarkovStateClassifier {
    pub fn classify_ambience(m: &StemMetrics) -> AmbienceState {
        if m.rms_db < -60.0 {
            return AmbienceState::Dry;
        }
        if m.rms_db < -40.0 {
            return AmbienceState::Subtle;
        }
        if m.rms_db >= -20.0 && m.crest_factor_db < 6.0 {
            return AmbienceState::Wash;
        }
        if m.rms_db >= -20.0 {
            return AmbienceState::Present;
        }
        AmbienceState::Lush
    }

    pub fn predict_next(state: AmbienceState) -> AmbienceState {
        let row = TRANSITION_MATRIX_AMBIENCE_V1[state as usize];
        let next_idx = row
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        match next_idx {
            0 => AmbienceState::Dry,
            1 => AmbienceState::Subtle,
            2 => AmbienceState::Present,
            3 => AmbienceState::Lush,
            _ => AmbienceState::Wash,
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
    fn classify_dry() {
        assert_eq!(
            AmbienceMarkovStateClassifier::classify_ambience(&mock(-65.0, 5.0)),
            AmbienceState::Dry
        );
    }

    #[test]
    fn predict_next_from_dry_is_dry() {
        assert_eq!(
            AmbienceMarkovStateClassifier::predict_next(AmbienceState::Dry),
            AmbienceState::Dry
        );
    }

    #[test]
    fn transition_matrix_rows_sum_to_one() {
        for row in &TRANSITION_MATRIX_AMBIENCE_V1 {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6, "Row sum != 1.0: {}", sum);
        }
    }
}
