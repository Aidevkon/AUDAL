use lineos_corpus::contract::CorpusEnvelope;

#[derive(Debug, Clone)]
pub struct UserSpatialProfile {
    pub width_tendency: f32,  // [0.0=narrow, 1.0=wide]
    pub depth_tendency: f32,  // [0.0=front, 1.0=deep]
    pub center_strength: f32, // [0.0=weak, 1.0=strong]
    pub lfe_tendency: f32,    // [0.0=no sub, 1.0=full sub]
    pub rear_decay: f32,      // how long tails go to rear
}

impl UserSpatialProfile {
    /// Derive spatial tendencies from corpus transitions.
    /// Many Lush/Wash → high depth_tendency
    /// Many Consonant/Transient → high width_tendency
    /// Many Tail transitions → high rear_decay
    /// INV-SP-1: deterministic
    pub fn from_corpus(envelopes: &[CorpusEnvelope]) -> Self {
        let mut lush_wash_count = 0;
        let mut cons_trans_count = 0;
        let mut tail_count = 0;
        let mut total_events = 0;

        for env in envelopes {
            for stem in &env.stems {
                for event in &stem.events {
                    total_events += 1;
                    match event.state.as_str() {
                        "lush" | "wash" => lush_wash_count += 1,
                        "consonant" | "transient" => cons_trans_count += 1,
                        "tail" => tail_count += 1,
                        _ => {}
                    }
                }
            }
        }

        if total_events == 0 {
            return Self::default_podcast();
        }

        let depth_tendency = (lush_wash_count as f32 / total_events as f32).min(1.0);
        let width_tendency = (cons_trans_count as f32 / total_events as f32).min(1.0);
        let rear_decay = (tail_count as f32 / total_events as f32).min(1.0);

        let center_strength = 0.5; // fallback
        let lfe_tendency = 0.2; // fallback

        Self {
            width_tendency,
            depth_tendency,
            center_strength,
            lfe_tendency,
            rear_decay,
        }
    }

    pub fn default_podcast() -> Self {
        Self {
            width_tendency: 0.3,
            depth_tendency: 0.2,
            center_strength: 0.8,
            lfe_tendency: 0.1,
            rear_decay: 0.3,
        }
    }

    pub fn default_music() -> Self {
        Self {
            width_tendency: 0.7,
            depth_tendency: 0.5,
            center_strength: 0.5,
            lfe_tendency: 0.4,
            rear_decay: 0.5,
        }
    }

    pub fn is_music(&self) -> bool {
        self.width_tendency >= 0.5
    }

    /// SP-P7: Apply Markov state prediction to modulate spatial params.
    /// Predicted Tail → increase rear_decay temporarily
    /// Predicted Consonant → increase width_tendency temporarily
    /// INV-SP-1: deterministic
    pub fn apply_markov_prediction(&self, predicted_voice_state: &str) -> Self {
        let mut modulated = self.clone();
        match predicted_voice_state {
            "tail" => modulated.rear_decay = (self.rear_decay + 0.2).min(1.0),
            "consonant" => modulated.width_tendency = (self.width_tendency + 0.15).min(1.0),
            "vowel" => modulated.center_strength = (self.center_strength + 0.1).min(1.0),
            "silence" => {
                modulated.width_tendency = (self.width_tendency - 0.1).max(0.0);
                modulated.rear_decay = (self.rear_decay - 0.1).max(0.0);
            }
            _ => {}
        }
        modulated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_podcast_has_strong_center() {
        let p = UserSpatialProfile::default_podcast();
        assert!(p.center_strength >= 0.8);
    }

    #[test]
    fn default_music_has_wide_tendencies() {
        let p = UserSpatialProfile::default_music();
        assert!(p.width_tendency >= 0.7);
    }

    #[test]
    fn markov_tail_increases_rear_decay() {
        let p = UserSpatialProfile::default_music();
        let m = p.apply_markov_prediction("tail");
        assert!(m.rear_decay > p.rear_decay);
    }

    #[test]
    fn markov_consonant_increases_width() {
        let p = UserSpatialProfile::default_music();
        let m = p.apply_markov_prediction("consonant");
        assert!(m.width_tendency > p.width_tendency);
    }

    #[test]
    fn markov_silence_reduces_activity() {
        let p = UserSpatialProfile::default_music();
        let m = p.apply_markov_prediction("silence");
        assert!(m.width_tendency < p.width_tendency);
        assert!(m.rear_decay < p.rear_decay);
    }

    #[test]
    fn all_fields_in_bounds_after_modulation() {
        let mut p = UserSpatialProfile::default_music();
        p.rear_decay = 0.95;
        let m = p.apply_markov_prediction("tail");
        assert!(m.rear_decay <= 1.0); // should clamp

        let mut p2 = UserSpatialProfile::default_music();
        p2.width_tendency = 0.05;
        let m2 = p2.apply_markov_prediction("silence");
        assert!(m2.width_tendency >= 0.0); // should clamp
    }

    #[test]
    fn profile_deterministic() {
        let p1 = UserSpatialProfile::default_music().apply_markov_prediction("tail");
        let p2 = UserSpatialProfile::default_music().apply_markov_prediction("tail");
        assert_eq!(p1.rear_decay, p2.rear_decay);
    }
}
