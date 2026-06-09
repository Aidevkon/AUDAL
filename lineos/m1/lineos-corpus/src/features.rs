//! StateFeatures — 5-feature vector per 100ms audio window.
//! Authority: corpus-learning-spec-v1_2.md CB-P1
//!
//! All features derived from existing sp314-dsp analysis.
//! Zero new mathematical dependencies.
//! no_std compatible — pure f32 arithmetic only.

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StateFeatures {
    /// RMS energy in dBFS. Range: [-144, 0].
    pub rms_db: f32,
    /// Change in RMS from previous window. THE KEY FEATURE.
    /// > +6.0 = Attack. < -3.0 = Decay. ±1.0 = Sustain.
    pub rms_delta: f32,
    /// Temporal transient density. Range: [0, 1].
    pub transient_density: f32,
    /// Spectral centroid in Hz. Range: [20, 20000].
    pub spectral_centroid: f32,
    /// Spectral flatness. Range: [0, 1].
    pub spectral_flatness: f32,
}

impl StateFeatures {
    pub fn new(
        rms_db:            f32,
        prev_rms_db:       f32,
        transient_density: f32,
        spectral_centroid: f32,
        spectral_flatness: f32,
    ) -> Self {
        Self {
            rms_db,
            rms_delta: rms_db - prev_rms_db,
            transient_density,
            spectral_centroid,
            spectral_flatness,
        }
    }

    pub fn activity_hint(&self) -> &'static str {
        if self.rms_db < -60.0        { "silence" }
        else if self.rms_delta > 6.0  { "attack"  }
        else if self.rms_delta < -3.0 { "decay"   }
        else                          { "sustain" }
    }
}

impl Default for StateFeatures {
    fn default() -> Self {
        Self {
            rms_db:            -144.0,
            rms_delta:         0.0,
            transient_density: 0.0,
            spectral_centroid: 1000.0,
            spectral_flatness: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_delta_computed_correctly() {
        let f = StateFeatures::new(-20.0, -26.0, 0.1, 2000.0, 0.3);
        assert!((f.rms_delta - 6.0).abs() < 0.001);
    }

    #[test]
    fn activity_hint_silence() {
        let f = StateFeatures::new(-80.0, -80.0, 0.0, 500.0, 0.1);
        assert_eq!(f.activity_hint(), "silence");
    }

    #[test]
    fn activity_hint_attack() {
        // rms_delta = -20 - -30 = +10 > 6
        let f = StateFeatures::new(-20.0, -30.0, 0.5, 3000.0, 0.4);
        assert_eq!(f.activity_hint(), "attack");
    }

    #[test]
    fn activity_hint_decay() {
        // rms_delta = -30 - -20 = -10 < -3
        let f = StateFeatures::new(-30.0, -20.0, 0.1, 2000.0, 0.2);
        assert_eq!(f.activity_hint(), "decay");
    }

    #[test]
    fn activity_hint_sustain() {
        // rms_delta = -20 - -20.5 = +0.5
        let f = StateFeatures::new(-20.0, -20.5, 0.1, 2000.0, 0.3);
        assert_eq!(f.activity_hint(), "sustain");
    }

    #[test]
    fn default_is_silence() {
        assert_eq!(StateFeatures::default().activity_hint(), "silence");
    }
}
