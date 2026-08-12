use crate::spatial::SpatialPreAnalysis;
use lineos_types::analysis::StemFeatures;

#[derive(Debug, Clone)]
pub struct ChannelAssignment {
    pub center_weight: f32,   // Voice → C [0.0, 1.0]
    pub front_lr_weight: f32, // Drums/Harmonics → L+R
    pub rear_lr_weight: f32,  // Ambience → Ls+Rs
    pub lfe_weight: f32,      // Bass sub → LFE
    pub side_weight: f32,     // stereo width contribution
}

#[derive(Debug, Clone)]
pub struct StemChannelAssignments {
    pub voice: ChannelAssignment,
    pub drums: ChannelAssignment,
    pub bass: ChannelAssignment,
    pub harmonics: ChannelAssignment,
    pub ambience: ChannelAssignment,
}

/// Το side_weight ΠΡΕΠΕΙ να μείνει κάτω από 1.0.
///
/// Το render_chunk γράφει (1.0 + s) αριστερά και
/// (1.0 - s) δεξιά. Με s > 1.0 ο δεύτερος όρος
/// γίνεται ΑΡΝΗΤΙΚΟΣ — δηλαδή το stem εμφανίζεται
/// ΑΝΤΕΣΤΡΑΜΜΕΝΟ στη μία πλευρά.
///
/// Αυτό δεν είναι θέμα ακραίας τιμής. Είναι
/// προϋπόθεση για ΚΑΘΕ φασικό στάδιο που μπαίνει
/// μετά: το matrixing εδώ είναι γραμμικό και
/// mono-safe (L+R = 2·stem·w, το s εξαφανίζεται στο
/// άθροισμα). Ένα allpass shuffler πάνω σε ήδη
/// αντεστραμμένο side δίνει απρόβλεπτο mono
/// fold-down.
///
/// 0.95 και όχι 1.0: στο 1.0 η μία πλευρά μηδενίζεται
/// εντελώς, που είναι hard pan και ακούγεται ως
/// σφάλμα.
#[inline]
fn clamp_side(s: f32) -> f32 {
    s.clamp(0.0, 0.95)
}

impl StemChannelAssignments {
    /// Deterministic assignment from StemFeatures + SpatialPreAnalysis
    /// INV-SP-8: Voice always has center_weight > 0
    pub fn compute(features: &StemFeatures, spatial: &SpatialPreAnalysis) -> Self {
        // ── Το πλάτος του ΥΛΙΚΟΥ κλιμακώνει το πλάτος του ΡΟΛΟΥ ──
        //
        // Το side_weight κάθε stem λέει ΠΟΙΟ απλώνεται
        // περισσότερο: ambience πάνω απ' όλα, μετά harmonics,
        // λίγο τα drums, καθόλου φωνή και μπάσο. Αυτό είναι
        // σωστό και μένει.
        //
        // Αυτό που έλειπε: ΠΟΣΟ πλατύ είναι το ίδιο το υλικό.
        // Μέχρι το 9334b54 το ms_ratio ήταν σταθερά μηδέν
        // (το SpatialPreAnalysis έπαιρνε δύο κλώνους του
        // ίδιου mono), οπότε δεν υπήρχε τίποτα να διαβαστεί.
        //
        // ΠΟΛΛΑΠΛΑΣΙΑΣΤΙΚΟ, ΟΧΙ ΠΡΟΣΘΕΤΙΚΟ: σε mono υλικό το
        // spread ΣΒΗΝΕΙ. Δεν κατασκευάζουμε στερεοφωνία που
        // δεν υπήρχε — αυτό είναι ρητή επιλογή του χρήστη
        // μέσω του intent knob, όχι σιωπηλή απόφαση του
        // engine.
        //
        // ΜΕΤΡΗΜΕΝΟ 2026-08-12, 24 κομμάτια σε 4 γένη:
        //   διάμεσος ms_ratio 0.2283
        //   pop 0.2056 · techno 0.2099 · acoustic 0.2254 ·
        //   metal 0.3012
        //   ακρότατα: Spastik 0.0442, Robot Rock 0.5018
        // Στον διάμεσο ο πολλαπλασιαστής είναι 1.0 και τα
        // βάρη ταυτίζονται με το 41710dd, που ακούστηκε και
        // ήταν σωστό.
        const MS_RATIO_MEDIAN: f32 = 0.23;
        let width_scale = spatial.ms_ratio / MS_RATIO_MEDIAN;

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
            side_weight: clamp_side(0.2 * width_scale),
        };

        // Bass rule: front + LFE
        let bass_lfe: f32 = if features.bass.spectral_centroid_hz < 80.0 {
            0.8
        } else {
            0.4
        };
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
            side_weight: clamp_side(0.4 * width_scale),
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
            side_weight: clamp_side(0.6 * width_scale),
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
    use lineos_types::analysis::{MixMetrics, StemFeatures, StemMetrics};

    fn mock_features() -> StemFeatures {
        StemFeatures {
            voice: StemMetrics {
                rms_db: -20.0,
                crest_factor_db: 8.0,
                ..Default::default()
            },
            drums: StemMetrics {
                rms_db: -12.0,
                crest_factor_db: 18.0,
                ..Default::default()
            },
            bass: StemMetrics {
                rms_db: -20.0,
                crest_factor_db: 6.0,
                spectral_centroid_hz: 60.0,
                ..Default::default()
            },
            harmonics: StemMetrics {
                rms_db: -22.0,
                crest_factor_db: 8.0,
                ..Default::default()
            },
            ambience: StemMetrics {
                rms_db: -30.0,
                crest_factor_db: 4.0,
                ..Default::default()
            },
            mix: MixMetrics::default(),
        }
    }

    fn mock_spatial() -> SpatialPreAnalysis {
        SpatialPreAnalysis {
            mid_energy: 0.5,
            side_energy: 0.1,
            ms_ratio: 0.2,
            transient_direction: 0.0,
            depth_score: 0.0,
            sub_energy: 0.1,
            presence_energy: 0.1,
            air_energy: 0.1,
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
