//! Bayesian inference engine — predict audio state from features.
//! Authority: corpus-learning-spec-v1_2.md CB-P5
//!
//! Combines:
//!   P(features | state)   — from EmissionHistogram (Bayes)
//!   P(state | prev_state) — from TransitionMatrix (Markov)
//!
//! Score = P(features|state) × P(state|prev_state)
//! Argmax over all states → predicted state + confidence.
//!
//! Replaces hardcoded confidence: 0.95 in corpus builder.

use crate::model::{TransitionMatrix, EmissionHistogram};
use crate::features::StateFeatures;
use std::collections::HashMap;

/// Prediction result from Bayesian inference.
#[derive(Debug, Clone)]
pub struct StatePrediction {
    /// Most likely state.
    pub state:      String,
    /// Normalized confidence [0, 1]. Real probability — not hardcoded.
    pub confidence: f32,
    /// All states ranked by score (state, score).
    pub scores:     Vec<(String, f32)>,
}

/// Complete model for one stem type within one preset.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StemMarkovModel {
    pub stem_type:   String,
    pub transitions: TransitionMatrix,
    /// state_label → EmissionHistogram
    pub emissions:   HashMap<String, EmissionHistogram>,
    pub n_sessions:  usize,
}

impl StemMarkovModel {
    pub fn new(stem_type: &str) -> Self {
        Self {
            stem_type:   stem_type.to_string(),
            transitions: TransitionMatrix::from_sequence(&[]),
            emissions:   HashMap::new(),
            n_sessions:  0,
        }
    }

    /// Train on a state sequence + corresponding features.
    /// sequences and features must be same length.
    pub fn train(
        &mut self,
        states:   &[String],
        features: &[StateFeatures],
    ) {
        // Update transition matrix
        let new_transitions = TransitionMatrix::from_sequence(states);
        self.transitions.merge(&new_transitions);

        // Update emission histograms
        for (state, feat) in states.iter().zip(features.iter()) {
            self.emissions
                .entry(state.clone())
                .or_insert_with(|| EmissionHistogram::new(state))
                .observe(feat);
        }

        self.n_sessions += 1;
    }
}

/// Predict the most likely state given features + previous state.
///
/// Score(state) = P(features|state) × P(state|prev_state)
/// Confidence = score / sum(all scores)  [normalized]
pub fn predict_state(
    features:   &StateFeatures,
    prev_state: &str,
    model:      &StemMarkovModel,
) -> StatePrediction {
    let states = &model.transitions.states;

    if states.is_empty() {
        return StatePrediction {
            state:      "unknown".to_string(),
            confidence: 0.0,
            scores:     vec![],
        };
    }

    let mut scores: Vec<(String, f32)> = states.iter().map(|state| {
        let p_emission = model.emissions
            .get(state)
            .map(|h| h.likelihood(features))
            .unwrap_or(1e-6); // small non-zero for unseen states

        let p_transition = model.transitions.probability(prev_state, state);

        (state.clone(), p_emission * p_transition)
    }).collect();

    // Sort descending by score
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let total: f32 = scores.iter().map(|(_, s)| s).sum();
    let confidence = if total > 0.0 {
        scores[0].1 / total
    } else {
        1.0 / states.len() as f32
    };

    StatePrediction {
        state:      scores[0].0.clone(),
        confidence,
        scores,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::StateFeatures;

    fn trained_model() -> StemMarkovModel {
        let mut model = StemMarkovModel::new("voice");
        let states = vec![
            "silence".to_string(), "vowel".to_string(),
            "consonant".to_string(), "tail".to_string(),
            "silence".to_string(), "vowel".to_string(),
            "vowel".to_string(), "tail".to_string(),
        ];
        let features: Vec<StateFeatures> = states.iter().map(|s| match s.as_str() {
            "silence"   => StateFeatures::new(-80.0, 0.0,  0.0, 1000.0, 0.5),
            "vowel"     => StateFeatures::new(-20.0, 5.0,  0.1, 2000.0, 0.2),
            "consonant" => StateFeatures::new(-25.0, 2.0,  0.3, 4000.0, 0.4),
            "tail"      => StateFeatures::new(-35.0, -8.0, 0.05, 1500.0, 0.3),
            _           => StateFeatures::default(),
        }).collect();
        model.train(&states, &features);
        model
    }

    #[test]
    fn predict_returns_valid_state() {
        let model = trained_model();
        let f = StateFeatures::new(-20.0, 5.0, 0.1, 2000.0, 0.2);
        let pred = predict_state(&f, "silence", &model);
        assert!(!pred.state.is_empty());
        assert!(pred.confidence >= 0.0 && pred.confidence <= 1.0);
    }

    #[test]
    fn confidence_sums_correctly() {
        let model = trained_model();
        let f = StateFeatures::new(-20.0, 5.0, 0.1, 2000.0, 0.2);
        let pred = predict_state(&f, "silence", &model);
        // Confidence must be normalized [0,1]
        assert!(pred.confidence <= 1.0 + 1e-5);
        assert!(pred.confidence >= 0.0);
    }

    #[test]
    fn scores_ordered_descending() {
        let model = trained_model();
        let f = StateFeatures::new(-20.0, 5.0, 0.1, 2000.0, 0.2);
        let pred = predict_state(&f, "silence", &model);
        for i in 1..pred.scores.len() {
            assert!(pred.scores[i-1].1 >= pred.scores[i].1,
                "Scores not sorted: {} < {}", pred.scores[i-1].1, pred.scores[i].1);
        }
    }

    #[test]
    fn empty_model_returns_unknown() {
        let model = StemMarkovModel::new("voice");
        let f = StateFeatures::default();
        let pred = predict_state(&f, "silence", &model);
        assert_eq!(pred.state, "unknown");
        assert_eq!(pred.confidence, 0.0);
    }

    #[test]
    fn train_increments_sessions() {
        let model = trained_model();
        assert_eq!(model.n_sessions, 1);
    }

    #[test]
    fn vowel_features_predict_vowel() {
        let model = trained_model();
        // Strong vowel features — rms high, delta moderate, low transient
        let f = StateFeatures::new(-20.0, 5.0, 0.05, 2000.0, 0.2);
        let pred = predict_state(&f, "silence", &model);
        // Most likely should be vowel (trained with these exact features)
        assert_eq!(pred.state, "vowel",
            "Expected vowel, got {} (confidence: {:.3})",
            pred.state, pred.confidence);
    }

    #[test]
    fn silence_features_predict_silence() {
        let model = trained_model();
        // Use silence features with prev_state="tail" —
        // training sequence has tail→silence transition.
        // Both Markov and Bayes agree on silence here.
        let f = StateFeatures::new(-80.0, 0.0, 0.0, 1000.0, 0.5);
        let pred = predict_state(&f, "tail", &model);
        assert_eq!(pred.state, "silence",
            "Expected silence, got {} (confidence: {:.3})",
            pred.state, pred.confidence);
    }
}
