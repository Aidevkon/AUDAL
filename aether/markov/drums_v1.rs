use lineos_types::analysis::StemMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumsState {
    Quiet = 0,
    BuildUp = 1,
    Transient = 2,
    Decay = 3,
    Sustain = 4,
}

impl DrumsState {
    pub fn to_str(&self) -> &'static str {
        match self {
            DrumsState::Quiet => "quiet",
            DrumsState::BuildUp => "buildup",
            DrumsState::Transient => "transient",
            DrumsState::Decay => "decay",
            DrumsState::Sustain => "sustain",
        }
    }
}

pub const TRANSITION_MATRIX_DRUMS_V1: [[f32; 5]; 5] = [
    [0.70, 0.15, 0.10, 0.05, 0.00],
    [0.10, 0.40, 0.40, 0.10, 0.00],
    [0.05, 0.00, 0.10, 0.70, 0.15],
    [0.10, 0.10, 0.05, 0.50, 0.25],
    [0.20, 0.10, 0.10, 0.30, 0.30],
];

pub struct DrumsMarkovStateClassifier;

impl DrumsMarkovStateClassifier {
    pub fn classify_drums(m: &StemMetrics) -> DrumsState {
        if m.rms_db < -60.0 {
            return DrumsState::Quiet;
        }
        if m.rms_db < -30.0 && m.crest_factor_db < 10.0 {
            return DrumsState::BuildUp;
        }
        if m.rms_db >= -30.0 && m.crest_factor_db > 15.0 {
            return DrumsState::Transient;
        }
        if m.rms_db >= -30.0 && m.crest_factor_db > 10.0 {
            return DrumsState::Decay;
        }
        DrumsState::Sustain
    }

    pub fn predict_next(state: DrumsState) -> DrumsState {
        let row = TRANSITION_MATRIX_DRUMS_V1[state as usize];
        let next_idx = row
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        match next_idx {
            0 => DrumsState::Quiet,
            1 => DrumsState::BuildUp,
            2 => DrumsState::Transient,
            3 => DrumsState::Decay,
            _ => DrumsState::Sustain,
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
    fn classify_quiet() {
        assert_eq!(
            DrumsMarkovStateClassifier::classify_drums(&mock(-65.0, 5.0)),
            DrumsState::Quiet
        );
    }

    #[test]
    fn predict_next_from_transient_is_decay() {
        assert_eq!(
            DrumsMarkovStateClassifier::predict_next(DrumsState::Transient),
            DrumsState::Decay
        );
    }

    #[test]
    fn transition_matrix_rows_sum_to_one() {
        for row in &TRANSITION_MATRIX_DRUMS_V1 {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6, "Row sum != 1.0: {}", sum);
        }
    }
}
