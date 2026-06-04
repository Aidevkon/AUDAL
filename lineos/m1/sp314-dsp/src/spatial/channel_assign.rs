use lineos_types::analysis::StemFeatures;
use crate::spatial::SpatialPreAnalysis;

pub struct ChannelAssignment {
    pub center_weight:    f32,  // Voice → C [0.0, 1.0]
    pub front_lr_weight:  f32,  // Drums/Harmonics → L+R
    pub rear_lr_weight:   f32,  // Ambience → Ls+Rs
    pub lfe_weight:       f32,  // Bass sub → LFE
    pub side_weight:      f32,  // stereo width contribution
}

pub struct StemChannelAssignments {
    pub voice:     ChannelAssignment,
    pub drums:     ChannelAssignment,
    pub bass:      ChannelAssignment,
    pub harmonics: ChannelAssignment,
    pub ambience:  ChannelAssignment,
}

impl StemChannelAssignments {
    /// Deterministic assignment from StemFeatures + SpatialPreAnalysis
    /// INV-SP-8: Voice always has center_weight > 0
    pub fn compute(features: &StemFeatures, spatial: &SpatialPreAnalysis) -> Self {
        // Voice center weight: stronger when signal is correlated
        // (correlated = mono-like = voice dominant = more center)
        let voice_center = if spatial.ms_ratio < 0.3 {
            // High correlation → strong center
            (0.8_f32 + (0.3 - spatial.ms_ratio) * 0.5).min(1.0_f32)
        } else {
            // Wide signal → less center
            (0.8_f32 - (spatial.ms_ratio - 0.3) * 0.3).max(0.3_f32)
        };
        
        let voice = ChannelAssignment {
            center_weight: voice_center.clamp(0.1, 1.0), // INV-SP-8: always > 0
            front_lr_weight: 0.2,
            rear_lr_weight: 0.0,
            lfe_weight: 0.0,
            side_weight: 0.0,
        };

        // Drums rule: front
        let drums = ChannelAssignment {
            center_weight: 0.0,
            front_lr_weight: 0.9,
            rear_lr_weight: 0.1,
            lfe_weight: 0.0,
            side_weight: 0.2,
        };

        // Bass rule: front + LFE
        let bass_lfe: f32 = if features.bass.spectral_centroid_hz < 80.0 { 0.8 } else { 0.4 };
        let bass = ChannelAssignment {
            center_weight: 0.0,
            front_lr_weight: 0.7,
            rear_lr_weight: 0.0,
            lfe_weight: bass_lfe.clamp(0.0, 1.0),
            side_weight: 0.0,
        };

        // Harmonics rule: front
        let harmonics = ChannelAssignment {
            center_weight: 0.0,
            front_lr_weight: 0.8,
            rear_lr_weight: 0.2,
            lfe_weight: 0.0,
            side_weight: 0.4,
        };

        // Ambience rule: rear
        // Ambience rear weight: stronger when signal is wide
        let ambience_rear = (0.3_f32 + spatial.ms_ratio * 0.5).min(0.8_f32);

        // Depth score drives rear channels
        let rear_factor = spatial.depth_score;
        let ambience = ChannelAssignment {
            center_weight: 0.0,
            front_lr_weight: 0.2,
            rear_lr_weight: (ambience_rear + rear_factor * 0.2).min(1.0_f32),
            lfe_weight: 0.0,
            side_weight: 0.6,
        };

        Self {
            voice,
            drums,
            bass,
            harmonics,
            ambience,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::analysis::{StemFeatures, StemMetrics, MixMetrics};

    fn mock_features() -> StemFeatures {
        StemFeatures {
            voice:     StemMetrics { rms_db: -20.0, crest_factor_db: 8.0, ..Default::default() },
            drums:     StemMetrics { rms_db: -12.0, crest_factor_db: 18.0, ..Default::default() },
            bass:      StemMetrics { rms_db: -20.0, crest_factor_db: 6.0, spectral_centroid_hz: 60.0, ..Default::default() },
            harmonics: StemMetrics { rms_db: -22.0, crest_factor_db: 8.0,  ..Default::default() },
            ambience:  StemMetrics { rms_db: -30.0, crest_factor_db: 4.0,  ..Default::default() },
            mix:       MixMetrics::default(),
        }
    }
    
    fn mock_spatial() -> SpatialPreAnalysis {
        SpatialPreAnalysis {
            mid_energy: 0.5, side_energy: 0.1, ms_ratio: 0.2,
            transient_direction: 0.0, depth_score: 0.0,
            sub_energy: 0.1, presence_energy: 0.1, air_energy: 0.1,
        }
    }

    #[test]
    fn voice_always_has_center_weight() {
        let f = mock_features();
        let s = mock_spatial();
        let assignments = StemChannelAssignments::compute(&f, &s);
        assert!(assignments.voice.center_weight > 0.0, "INV-SP-8 violated");
    }

    #[test]
    fn ambience_goes_to_rear() {
        let f = mock_features();
        let s = mock_spatial();
        let assignments = StemChannelAssignments::compute(&f, &s);
        assert!(assignments.ambience.rear_lr_weight > assignments.ambience.front_lr_weight);
    }

    #[test]
    fn bass_has_lfe_weight() {
        let f = mock_features();
        let s = mock_spatial();
        let assignments = StemChannelAssignments::compute(&f, &s);
        assert!(assignments.bass.lfe_weight > 0.0);
    }

    #[test]
    fn drums_go_to_front() {
        let f = mock_features();
        let s = mock_spatial();
        let assignments = StemChannelAssignments::compute(&f, &s);
        assert!(assignments.drums.front_lr_weight > assignments.drums.rear_lr_weight);
    }

    #[test]
    fn all_weights_in_bounds() {
        let f = mock_features();
        let s = mock_spatial();
        let a = StemChannelAssignments::compute(&f, &s);
        
        let check = |ca: &ChannelAssignment| {
            assert!((0.0..=1.0).contains(&ca.center_weight));
            assert!((0.0..=1.0).contains(&ca.front_lr_weight));
            assert!((0.0..=1.0).contains(&ca.rear_lr_weight));
            assert!((0.0..=1.0).contains(&ca.lfe_weight));
            assert!((0.0..=1.0).contains(&ca.side_weight));
        };
        check(&a.voice);
        check(&a.drums);
        check(&a.bass);
        check(&a.harmonics);
        check(&a.ambience);
    }

    #[test]
    fn assignment_deterministic() {
        let f = mock_features();
        let s = mock_spatial();
        let a1 = StemChannelAssignments::compute(&f, &s);
        let a2 = StemChannelAssignments::compute(&f, &s);
        assert_eq!(a1.voice.center_weight, a2.voice.center_weight);
        assert_eq!(a1.bass.lfe_weight, a2.bass.lfe_weight);
    }
}
