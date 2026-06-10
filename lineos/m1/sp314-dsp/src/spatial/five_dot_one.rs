use crate::spatial::channel_assign::StemChannelAssignments;
use crate::stft::stem_renderer::FiveStems;

pub struct SpatialFirewall {
    pub max_rear_energy: f32,  // 0.4
    pub max_lfe_db: f32,       // -6.0
    pub max_center_bleed: f32, // 0.3
}

impl Default for SpatialFirewall {
    fn default() -> Self {
        Self {
            max_rear_energy: 0.4,
            max_lfe_db: -6.0,
            max_center_bleed: 0.3,
        }
    }
}

impl SpatialFirewall {
    pub fn apply(&self, stage: &mut FiveDotOneStage) {
        // Clamp rear energy ≤ 40% of front (INV-SP-7)
        let mut front_energy_sq = 0.0;
        let mut rear_energy_sq = 0.0;

        let len = stage.l.len();
        if len == 0 {
            return;
        }

        for i in 0..len {
            front_energy_sq +=
                stage.l[i] * stage.l[i] + stage.r[i] * stage.r[i] + stage.c[i] * stage.c[i];
            rear_energy_sq += stage.ls[i] * stage.ls[i] + stage.rs[i] * stage.rs[i];
        }

        let front_energy = front_energy_sq.sqrt();
        let rear_energy = rear_energy_sq.sqrt();

        if front_energy > 1e-6 && rear_energy > 1e-6 {
            let ratio = rear_energy / front_energy;
            if ratio > self.max_rear_energy {
                let scale = self.max_rear_energy / ratio;
                for i in 0..len {
                    stage.ls[i] *= scale;
                    stage.rs[i] *= scale;
                }
            }
        }

        // Clamp LFE ≤ -6dB (INV-SP-6)
        let max_lfe_linear = 10.0_f32.powf(self.max_lfe_db / 20.0);
        let mut lfe_max = 0.0_f32;
        for i in 0..len {
            lfe_max = lfe_max.max(stage.lfe[i].abs());
        }

        if lfe_max > max_lfe_linear {
            let scale = max_lfe_linear / lfe_max;
            for i in 0..len {
                stage.lfe[i] *= scale;
            }
        }
    }
}

pub struct FiveDotOneStage {
    pub l: Vec<f32>,   // Front Left
    pub r: Vec<f32>,   // Front Right
    pub c: Vec<f32>,   // Center
    pub ls: Vec<f32>,  // Surround Left
    pub rs: Vec<f32>,  // Surround Right
    pub lfe: Vec<f32>, // Sub
}

impl FiveDotOneStage {
    /// Mix FiveStems into 6 channels using StemChannelAssignments.
    /// INV-SP-1: deterministic
    /// INV-SP-5: always active regardless of renderer
    pub fn render(
        stems: &FiveStems,
        assignments: &StemChannelAssignments,
        firewall: &SpatialFirewall,
    ) -> Self {
        let len = stems.voice.len();
        let mut l = vec![0.0; len];
        let mut r = vec![0.0; len];
        let mut c = vec![0.0; len];
        let mut ls = vec![0.0; len];
        let mut rs = vec![0.0; len];
        let mut lfe = vec![0.0; len];

        for i in 0..len {
            let v = stems.voice[i];
            let d = stems.drums[i];
            let b = stems.bass[i];
            let h = stems.harmonics[i];
            let a = stems.ambience[i];

            c[i] += v * assignments.voice.center_weight
                + d * assignments.drums.center_weight
                + b * assignments.bass.center_weight
                + h * assignments.harmonics.center_weight
                + a * assignments.ambience.center_weight;
            l[i] += v * assignments.voice.front_lr_weight
                + d * assignments.drums.front_lr_weight
                + b * assignments.bass.front_lr_weight
                + h * assignments.harmonics.front_lr_weight
                + a * assignments.ambience.front_lr_weight;
            r[i] += v * assignments.voice.front_lr_weight
                + d * assignments.drums.front_lr_weight
                + b * assignments.bass.front_lr_weight
                + h * assignments.harmonics.front_lr_weight
                + a * assignments.ambience.front_lr_weight;
            ls[i] += v * assignments.voice.rear_lr_weight
                + d * assignments.drums.rear_lr_weight
                + b * assignments.bass.rear_lr_weight
                + h * assignments.harmonics.rear_lr_weight
                + a * assignments.ambience.rear_lr_weight;
            rs[i] += v * assignments.voice.rear_lr_weight
                + d * assignments.drums.rear_lr_weight
                + b * assignments.bass.rear_lr_weight
                + h * assignments.harmonics.rear_lr_weight
                + a * assignments.ambience.rear_lr_weight;
            lfe[i] += v * assignments.voice.lfe_weight
                + d * assignments.drums.lfe_weight
                + b * assignments.bass.lfe_weight
                + h * assignments.harmonics.lfe_weight
                + a * assignments.ambience.lfe_weight;
        }

        let mut stage = Self {
            l,
            r,
            c,
            ls,
            rs,
            lfe,
        };
        firewall.apply(&mut stage);
        stage
    }

    /// Chunk version of render() — same math, smaller allocation.
    /// Uses pre-locked StemChannelAssignments from ScoutResult.
    /// INV-SP-1: deterministic — same chunk → same output.
    pub fn render_chunk(
        voice: &[f32],
        drums: &[f32],
        bass: &[f32],
        harmonics: &[f32],
        ambience: &[f32],
        assignments: &StemChannelAssignments,
    ) -> Self {
        let len = voice
            .len()
            .min(drums.len())
            .min(bass.len())
            .min(harmonics.len())
            .min(ambience.len());

        let mut l = vec![0.0_f32; len];
        let mut r = vec![0.0_f32; len];
        let mut c = vec![0.0_f32; len];
        let mut ls = vec![0.0_f32; len];
        let mut rs = vec![0.0_f32; len];
        let mut lfe = vec![0.0_f32; len];

        for i in 0..len {
            let v = voice[i];
            let d = drums[i];
            let b = bass[i];
            let h = harmonics[i];
            let a = ambience[i];

            c[i] = v * assignments.voice.center_weight
                + d * assignments.drums.center_weight
                + b * assignments.bass.center_weight
                + h * assignments.harmonics.center_weight
                + a * assignments.ambience.center_weight;

            l[i] = v * assignments.voice.front_lr_weight
                + d * assignments.drums.front_lr_weight
                + b * assignments.bass.front_lr_weight
                + h * assignments.harmonics.front_lr_weight
                + a * assignments.ambience.front_lr_weight;

            r[i] = l[i]; // symmetric front

            ls[i] = v * assignments.voice.rear_lr_weight
                + d * assignments.drums.rear_lr_weight
                + b * assignments.bass.rear_lr_weight
                + h * assignments.harmonics.rear_lr_weight
                + a * assignments.ambience.rear_lr_weight;

            rs[i] = ls[i]; // symmetric rear

            lfe[i] = v * assignments.voice.lfe_weight
                + d * assignments.drums.lfe_weight
                + b * assignments.bass.lfe_weight
                + h * assignments.harmonics.lfe_weight
                + a * assignments.ambience.lfe_weight;
        }

        Self {
            l,
            r,
            c,
            ls,
            rs,
            lfe,
        }
    }

    /// Apply pre-computed firewall scales to a chunk.
    /// Scales computed once in Pass 1 from proxy stems.
    /// INV-SP-7: rear ≤ 40% of front (enforced by scale).
    /// INV-SP-6: LFE ≤ -6dB (enforced by scale).
    pub fn apply_scales(&mut self, rear_scale: f32, lfe_scale: f32) {
        for i in 0..self.ls.len() {
            self.ls[i] *= rear_scale;
            self.rs[i] *= rear_scale;
            self.lfe[i] *= lfe_scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::channel_assign::ChannelAssignment;

    fn default_firewall() -> SpatialFirewall {
        SpatialFirewall {
            max_rear_energy: 0.4,
            max_lfe_db: -6.0,
            max_center_bleed: 0.3,
        }
    }

    fn empty_assignments() -> StemChannelAssignments {
        let zero = || ChannelAssignment {
            center_weight: 0.0,
            front_lr_weight: 0.0,
            rear_lr_weight: 0.0,
            lfe_weight: 0.0,
            side_weight: 0.0,
        };
        StemChannelAssignments {
            voice: zero(),
            drums: zero(),
            bass: zero(),
            harmonics: zero(),
            ambience: zero(),
        }
    }

    fn mock_stems() -> FiveStems {
        FiveStems {
            voice: vec![1.0; 100],
            drums: vec![0.0; 100],
            bass: vec![0.0; 100],
            harmonics: vec![0.0; 100],
            ambience: vec![0.0; 100],
            voice_transient_density: 0.0,
            drums_transient_density: 0.0,
            bass_transient_density: 0.0,
            harmonics_transient_density: 0.0,
            ambience_transient_density: 0.0,
        }
    }

    #[test]
    fn voice_energy_in_center_channel() {
        let mut a = empty_assignments();
        a.voice.center_weight = 1.0;
        let stems = mock_stems();
        let stage = FiveDotOneStage::render(&stems, &a, &default_firewall());
        assert!(stage.c.iter().sum::<f32>() > 0.0);
    }

    #[test]
    fn ambience_energy_in_rear_channels() {
        let mut a = empty_assignments();
        a.ambience.rear_lr_weight = 1.0;
        let mut stems = mock_stems();
        stems.ambience = vec![1.0; 100];
        let stage = FiveDotOneStage::render(&stems, &a, &default_firewall());
        assert!(stage.ls.iter().sum::<f32>() > 0.0);
        assert!(stage.rs.iter().sum::<f32>() > 0.0);
    }

    #[test]
    fn lfe_within_bounds() {
        let mut a = empty_assignments();
        a.bass.lfe_weight = 1.0;
        let mut stems = mock_stems();
        stems.bass = vec![1.0; 100]; // 0 dBFS input
        let stage = FiveDotOneStage::render(&stems, &a, &default_firewall());

        let max_lfe = stage.lfe.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
        let max_lfe_db = 20.0 * max_lfe.log10();
        // INV-SP-6 limits to -6 dB
        assert!(max_lfe_db <= -5.99);
    }

    #[test]
    fn rear_energy_within_bounds() {
        let mut a = empty_assignments();
        // Force massive rear energy
        a.ambience.rear_lr_weight = 100.0;
        a.voice.front_lr_weight = 1.0;
        let mut stems = mock_stems();
        stems.ambience = vec![1.0; 100];
        stems.voice = vec![1.0; 100];
        let stage = FiveDotOneStage::render(&stems, &a, &default_firewall());

        let front_energy =
            stage.l.iter().map(|v| v * v).sum::<f32>() + stage.r.iter().map(|v| v * v).sum::<f32>();
        let rear_energy = stage.ls.iter().map(|v| v * v).sum::<f32>()
            + stage.rs.iter().map(|v| v * v).sum::<f32>();
        let ratio = rear_energy.sqrt() / front_energy.sqrt();
        assert!(ratio <= 0.401, "Rear energy ratio was {}", ratio);
    }

    #[test]
    fn render_deterministic() {
        let mut a = empty_assignments();
        a.voice.center_weight = 1.0;
        let stems = mock_stems();
        let fw = default_firewall();
        let stage1 = FiveDotOneStage::render(&stems, &a, &fw);
        let stage2 = FiveDotOneStage::render(&stems, &a, &fw);
        assert_eq!(stage1.c, stage2.c);
    }
}
