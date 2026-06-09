//! Markov transition matrix and emission histogram.
//! Authority: corpus-learning-spec-v1_2.md CB-P3 + CB-P4

use std::collections::HashMap;

/// N×N transition probability matrix.
/// P[i][j] = P(next_state=j | current_state=i)
/// States indexed alphabetically for determinism.
/// Training: count consecutive pairs, normalize rows.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TransitionMatrix {
    /// State labels — sorted alphabetically for determinism.
    pub states:        Vec<String>,
    /// Raw counts[from_idx][to_idx].
    pub counts:        Vec<Vec<usize>>,
    /// Normalized probabilities[from_idx][to_idx]. Row sums to 1.0.
    pub probs:         Vec<Vec<f32>>,
    /// Total number of transitions observed.
    pub n_transitions: usize,
}

impl TransitionMatrix {
    /// Build from a state sequence.
    /// Counts every consecutive (current, next) pair.
    /// States sorted alphabetically for cross-platform determinism.
    pub fn from_sequence(sequence: &[String]) -> Self {
        // Collect unique states, sort for determinism
        let mut state_set: Vec<String> = sequence.iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        state_set.sort();

        let n = state_set.len();
        if n == 0 {
            return Self {
                states: vec![],
                counts: vec![],
                probs:  vec![],
                n_transitions: 0,
            };
        }

        // Index lookup
        let idx: HashMap<&str, usize> = state_set.iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();

        let mut counts = vec![vec![0usize; n]; n];
        let mut n_transitions = 0usize;

        for window in sequence.windows(2) {
            let from = idx[window[0].as_str()];
            let to   = idx[window[1].as_str()];
            counts[from][to] += 1;
            n_transitions += 1;
        }

        let probs = normalize_rows(&counts);

        Self { states: state_set, counts, probs, n_transitions }
    }

    /// Merge another matrix into self (incremental training).
    /// Adds raw counts then renormalizes.
    /// INV-CB-2: incremental — never full retrain.
    pub fn merge(&mut self, other: &TransitionMatrix) {
        // Collect union of states
        let mut all_states: Vec<String> = self.states.iter()
            .chain(other.states.iter())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        all_states.sort();

        let n = all_states.len();
        let idx: HashMap<&str, usize> = all_states.iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();

        let mut new_counts = vec![vec![0usize; n]; n];

        // Copy self counts
        for (fi, from) in self.states.iter().enumerate() {
            for (ti, to) in self.states.iter().enumerate() {
                let new_fi = idx[from.as_str()];
                let new_ti = idx[to.as_str()];
                new_counts[new_fi][new_ti] += self.counts[fi][ti];
            }
        }

        // Add other counts
        for (fi, from) in other.states.iter().enumerate() {
            for (ti, to) in other.states.iter().enumerate() {
                let new_fi = idx[from.as_str()];
                let new_ti = idx[to.as_str()];
                new_counts[new_fi][new_ti] += other.counts[fi][ti];
            }
        }

        self.states        = all_states;
        self.counts        = new_counts;
        self.n_transitions = self.n_transitions + other.n_transitions;
        self.probs         = normalize_rows(&self.counts);
    }

    /// P(next_state | current_state).
    /// Returns uniform probability if state unseen (Laplace smoothing).
    pub fn probability(&self, from: &str, to: &str) -> f32 {
        let n = self.states.len();
        if n == 0 { return 0.0; }

        let from_idx = self.states.iter().position(|s| s == from);
        let to_idx   = self.states.iter().position(|s| s == to);

        match (from_idx, to_idx) {
            (Some(fi), Some(ti)) => self.probs[fi][ti],
            _ => 1.0 / n as f32, // uniform for unseen states
        }
    }
}

/// Normalize each row to sum to 1.0.
/// Rows with all-zero counts get uniform distribution.
fn normalize_rows(counts: &[Vec<usize>]) -> Vec<Vec<f32>> {
    counts.iter().map(|row| {
        let total: usize = row.iter().sum();
        if total == 0 {
            let n = row.len();
            vec![if n > 0 { 1.0 / n as f32 } else { 0.0 }; n]
        } else {
            row.iter().map(|&c| c as f32 / total as f32).collect()
        }
    }).collect()
}

pub const RMS_DB_BINS:            [f32; 4] = [-80.0, -50.0, -30.0, -10.0];
pub const RMS_DELTA_BINS:         [f32; 4] = [-10.0, -2.0,   2.0,  10.0];
pub const TRANSIENT_DENSITY_BINS: [f32; 3] = [0.05,  0.15,  0.30];
pub const SPECTRAL_CENTROID_BINS: [f32; 3] = [500.0, 2000.0, 6000.0];
pub const SPECTRAL_FLATNESS_BINS: [f32; 2] = [0.1,   0.5];

/// Find bin index. Bin 0 = below first boundary.
/// Uses strict < so boundary values fall into upper bin.
fn bin_index(value: f32, boundaries: &[f32]) -> usize {
    boundaries.iter().position(|&b| value < b).unwrap_or(boundaries.len())
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EmissionHistogram {
    pub state:             String,
    pub rms_db:            [usize; 5],
    pub rms_delta:         [usize; 5],
    pub transient_density: [usize; 4],
    pub spectral_centroid: [usize; 4],
    pub spectral_flatness: [usize; 3],
    pub total:             usize,
}

impl EmissionHistogram {
    pub fn new(state: &str) -> Self {
        Self {
            state:             state.to_string(),
            rms_db:            [0; 5],
            rms_delta:         [0; 5],
            transient_density: [0; 4],
            spectral_centroid: [0; 4],
            spectral_flatness: [0; 3],
            total:             0,
        }
    }

    pub fn observe(&mut self, features: &crate::features::StateFeatures) {
        self.rms_db[bin_index(features.rms_db,
            &RMS_DB_BINS)] += 1;
        self.rms_delta[bin_index(features.rms_delta,
            &RMS_DELTA_BINS)] += 1;
        self.transient_density[bin_index(features.transient_density,
            &TRANSIENT_DENSITY_BINS)] += 1;
        self.spectral_centroid[bin_index(features.spectral_centroid,
            &SPECTRAL_CENTROID_BINS)] += 1;
        self.spectral_flatness[bin_index(features.spectral_flatness,
            &SPECTRAL_FLATNESS_BINS)] += 1;
        self.total += 1;
    }

    /// P(features | state) — product of bin probabilities (Naive Bayes).
    /// Returns 0.0 if no observations yet.
    pub fn likelihood(&self, features: &crate::features::StateFeatures) -> f32 {
        if self.total == 0 { return 0.0; }
        bin_prob(&self.rms_db,
            bin_index(features.rms_db, &RMS_DB_BINS))
        * bin_prob(&self.rms_delta,
            bin_index(features.rms_delta, &RMS_DELTA_BINS))
        * bin_prob(&self.transient_density,
            bin_index(features.transient_density, &TRANSIENT_DENSITY_BINS))
        * bin_prob(&self.spectral_centroid,
            bin_index(features.spectral_centroid, &SPECTRAL_CENTROID_BINS))
        * bin_prob(&self.spectral_flatness,
            bin_index(features.spectral_flatness, &SPECTRAL_FLATNESS_BINS))
    }

    pub fn merge(&mut self, other: &EmissionHistogram) {
        for i in 0..5 { self.rms_db[i]    += other.rms_db[i]; }
        for i in 0..5 { self.rms_delta[i] += other.rms_delta[i]; }
        for i in 0..4 { self.transient_density[i] += other.transient_density[i]; }
        for i in 0..4 { self.spectral_centroid[i] += other.spectral_centroid[i]; }
        for i in 0..3 { self.spectral_flatness[i] += other.spectral_flatness[i]; }
        self.total += other.total;
    }
}

/// Bin probability with Laplace smoothing (+1 to each bin).
fn bin_prob(counts: &[usize], bin: usize) -> f32 {
    let total = counts.iter().sum::<usize>() + counts.len();
    let count = counts.get(bin).copied().unwrap_or(0) + 1;
    count as f32 / total as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_sequence_counts_correctly() {
        let seq = vec![
            "a".to_string(), "b".to_string(), "a".to_string(),
            "b".to_string(), "c".to_string(),
        ];
        let m = TransitionMatrix::from_sequence(&seq);
        assert_eq!(m.n_transitions, 4);
        // a→b twice, b→a once, b→c once
        assert!((m.probability("a", "b") - 1.0).abs() < 0.001);
        assert!((m.probability("b", "a") - 0.5).abs() < 0.001);
        assert!((m.probability("b", "c") - 0.5).abs() < 0.001);
    }

    #[test]
    fn from_empty_sequence() {
        let m = TransitionMatrix::from_sequence(&[]);
        assert_eq!(m.n_transitions, 0);
        assert!(m.states.is_empty());
    }

    #[test]
    fn from_single_element() {
        let seq = vec!["silence".to_string()];
        let m = TransitionMatrix::from_sequence(&seq);
        assert_eq!(m.n_transitions, 0);
        assert_eq!(m.states.len(), 1);
    }

    #[test]
    fn states_sorted_alphabetically() {
        let seq = vec![
            "vowel".to_string(), "consonant".to_string(),
            "tail".to_string(), "silence".to_string(),
        ];
        let m = TransitionMatrix::from_sequence(&seq);
        assert_eq!(m.states, vec!["consonant", "silence", "tail", "vowel"]);
    }

    #[test]
    fn merge_combines_counts() {
        let seq1 = vec!["a".to_string(), "b".to_string()];
        let seq2 = vec!["a".to_string(), "b".to_string()];
        let mut m1 = TransitionMatrix::from_sequence(&seq1);
        let m2     = TransitionMatrix::from_sequence(&seq2);
        m1.merge(&m2);
        assert_eq!(m1.n_transitions, 2);
        // a→b should be count=2, prob=1.0
        assert!((m1.probability("a", "b") - 1.0).abs() < 0.001);
    }

    #[test]
    fn unseen_state_returns_uniform() {
        let seq = vec!["a".to_string(), "b".to_string()];
        let m = TransitionMatrix::from_sequence(&seq);
        // "z" is unseen → uniform = 1/2 = 0.5
        let p = m.probability("z", "a");
        assert!((p - 0.5).abs() < 0.001);
    }

    #[test]
    fn row_sums_to_one() {
        let seq = vec![
            "silence".to_string(), "vowel".to_string(),
            "consonant".to_string(), "tail".to_string(),
            "silence".to_string(), "breath".to_string(),
        ];
        let m = TransitionMatrix::from_sequence(&seq);
        for (i, from) in m.states.iter().enumerate() {
            let row_sum: f32 = m.probs[i].iter().sum();
            assert!((row_sum - 1.0).abs() < 0.001,
                "Row {} ({}) sums to {}", i, from, row_sum);
        }
    }

    #[test]
    fn emission_observe_increments_bins() {
        use crate::features::StateFeatures;
        let mut h = EmissionHistogram::new("vowel");
        let f = StateFeatures::new(-20.0, 0.0, 0.1, 2000.0, 0.3);
        h.observe(&f);
        assert_eq!(h.total, 1);
        // rms_db=-20 → bin 3 (>= -30, < -10)
        assert_eq!(h.rms_db[3], 1);
    }

    #[test]
    fn emission_likelihood_nonzero_after_observation() {
        use crate::features::StateFeatures;
        let mut h = EmissionHistogram::new("vowel");
        let f = StateFeatures::new(-20.0, 0.0, 0.1, 2000.0, 0.3);
        h.observe(&f);
        let p = h.likelihood(&f);
        assert!(p > 0.0, "likelihood must be > 0 after observation");
        assert!(p <= 1.0, "likelihood must be <= 1.0");
    }

    #[test]
    fn emission_empty_returns_zero() {
        use crate::features::StateFeatures;
        let h = EmissionHistogram::new("silence");
        assert_eq!(h.likelihood(&StateFeatures::default()), 0.0);
    }

    #[test]
    fn emission_merge_sums_counts() {
        use crate::features::StateFeatures;
        let f = StateFeatures::new(-20.0, 0.0, 0.1, 2000.0, 0.3);
        let mut h1 = EmissionHistogram::new("vowel");
        let mut h2 = EmissionHistogram::new("vowel");
        h1.observe(&f);
        h2.observe(&f);
        h1.merge(&h2);
        assert_eq!(h1.total, 2);
        assert_eq!(h1.rms_db[3], 2);
    }

    #[test]
    fn bin_index_boundaries() {
        assert_eq!(bin_index(-144.0, &RMS_DB_BINS), 0); // < -80 → bin 0
        assert_eq!(bin_index(-80.0,  &RMS_DB_BINS), 1); // not < -80 → bin 1
        assert_eq!(bin_index(0.0,    &RMS_DB_BINS), 4); // above all → bin 4
    }
}
