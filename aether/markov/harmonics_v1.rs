use lineos_types::analysis::StemMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarmonicsState {
    Silent = 0,
    Sparse = 1,
    Dense  = 2,
    Bright = 3,
    Warm   = 4,
}

pub const TRANSITION_MATRIX_HARMONICS_V1: [[f32; 5]; 5] = [
    [0.70, 0.20, 0.05, 0.05, 0.00],
    [0.10, 0.60, 0.10, 0.10, 0.10],
    [0.05, 0.10, 0.60, 0.10, 0.15],
    [0.05, 0.10, 0.15, 0.50, 0.20],
    [0.10, 0.10, 0.15, 0.15, 0.50],
];

pub struct HarmonicsMarkovStateClassifier;

impl HarmonicsMarkovStateClassifier {
    pub fn classify_harmonics(m: &StemMetrics) -> HarmonicsState {
        if m.rms_db < -60.0 {
            return HarmonicsState::Silent;
        }
        if m.rms_db < -40.0 {
            return HarmonicsState::Sparse;
        }
        if m.crest_factor_db > 15.0 {
            return HarmonicsState::Bright;
        }
        if m.rms_db > -20.0 {
            return HarmonicsState::Dense;
        }
        HarmonicsState::Warm
    }

    pub fn predict_next(state: HarmonicsState) -> HarmonicsState {
        let row = TRANSITION_MATRIX_HARMONICS_V1[state as usize];
        let next_idx = row.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        match next_idx {
            0 => HarmonicsState::Silent,
            1 => HarmonicsState::Sparse,
            2 => HarmonicsState::Dense,
            3 => HarmonicsState::Bright,
            _ => HarmonicsState::Warm,
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
    fn classify_silent() {
        assert_eq!(HarmonicsMarkovStateClassifier::classify_harmonics(&mock(-65.0, 5.0)), HarmonicsState::Silent);
    }

    #[test]
    fn predict_next_from_sparse_is_sparse() {
        assert_eq!(HarmonicsMarkovStateClassifier::predict_next(HarmonicsState::Sparse), HarmonicsState::Sparse);
    }

    #[test]
    fn transition_matrix_rows_sum_to_one() {
        for row in &TRANSITION_MATRIX_HARMONICS_V1 {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6, "Row sum != 1.0: {}", sum);
        }
    }
}
