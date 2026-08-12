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

        let front_energy = libm::sqrtf(front_energy_sq);
        let rear_energy = libm::sqrtf(rear_energy_sq);

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
        let max_lfe_linear = libm::powf(10.0_f32, self.max_lfe_db / 20.0);
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
    pub fn deterministic_upmix(
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
        firewall: &SpatialFirewall,
    ) -> Self {
        use crate::compressor::crossover::CrossoverLR4;
        use crate::spatial::all_pass::AllPassFilter;

        let n = left.len();
        let mut c = Vec::with_capacity(n);
        let mut ls = Vec::with_capacity(n);
        let mut rs = Vec::with_capacity(n);
        let mut lfe = Vec::with_capacity(n);

        let mut side_xover = CrossoverLR4::new(200.0, sample_rate);
        let mut mid_xover = CrossoverLR4::new(80.0, sample_rate);
        let mut rs_allpass = AllPassFilter::new(1000.0, 0.707, sample_rate);

        for i in 0..n {
            // Inline M/S — avoids interleave/de-interleave overhead that calling
            // MidSideMatrix::encode() (which expects one interleaved buffer) would
            // require here; same choice made in xaak's telemetry_worker this morning.
            let mid = (left[i] + right[i]) * 0.5;
            let side = (left[i] - right[i]) * 0.5;

            let (_side_low, side_high) = side_xover.process(side);
            let (mid_low, _mid_high) = mid_xover.process(mid);

            c.push(mid * 0.707);
            ls.push(side_high);
            rs.push(rs_allpass.process(-side_high));
            lfe.push(mid_low * 0.5);
        }

        let mut stage = Self {
            l: left.to_vec(),
            r: right.to_vec(),
            c,
            ls,
            rs,
            lfe,
        };
        firewall.apply(&mut stage);
        stage
    }

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

            // Το side_weight απλώνει το stem ΑΣΥΜΜΕΤΡΑ. Κάθε
            // stem παίρνει διαφορετικό gain ανά πλευρά — το
            // πλάτος προκύπτει από ΔΙΑΦΟΡΕΤΙΚΟ ΠΕΡΙΕΧΟΜΕΝΟ
            // αριστερά και δεξιά, όχι από φασική επεξεργασία
            // του ίδιου σήματος.
            //
            // ΜΕΤΡΗΜΕΝΟ (stem_independence.rs): harmonics και
            // ambience έχουν συσχέτιση 0.2790 — χωρίζουν καθαρά.
            // bass↔harmonics 0.6192, γι' αυτό το bass έχει
            // side 0.0 και μένει κέντρο.
            //
            // ΗΤΑΝ r[i] = l[i] από το f98166e, με σχόλιο
            // "symmetric front". Συνέπεια: κάθε music master
            // mono — το fold-down άθροιζε δύο ταυτόσημα κανάλια,
            // και ο Glue widener (Reveal) δεν είχε side να δει.

            let sv = assignments.voice.side_weight;
            let sd = assignments.drums.side_weight;
            let sb = assignments.bass.side_weight;
            let sh = assignments.harmonics.side_weight;
            let sa = assignments.ambience.side_weight;

            // Το side_weight λέει ΠΟΣΟ απλώνεται το stem, το pan
            // ΠΡΟΣ ΤΑ ΠΟΥ. Μέχρι τώρα η κατεύθυνση ήταν
            // κωδικοποιημένη στη ΣΕΙΡΑ των όρων — το ambience
            // είχε αντεστραμμένα πρόσημα και τα υπόλοιπα όχι.
            // Ίδια αριθμητική, ρητή πλέον.
            let pv = assignments.voice.side_weight * assignments.voice.pan;
            let pd = assignments.drums.side_weight * assignments.drums.pan;
            let pb = assignments.bass.side_weight * assignments.bass.pan;
            let ph = assignments.harmonics.side_weight * assignments.harmonics.pan;
            let pa = assignments.ambience.side_weight * assignments.ambience.pan;

            l[i] = v * assignments.voice.front_lr_weight * (1.0 + pv)
                + d * assignments.drums.front_lr_weight * (1.0 + pd)
                + b * assignments.bass.front_lr_weight * (1.0 + pb)
                + h * assignments.harmonics.front_lr_weight * (1.0 + ph)
                + a * assignments.ambience.front_lr_weight * (1.0 + pa);

            r[i] = v * assignments.voice.front_lr_weight * (1.0 - pv)
                + d * assignments.drums.front_lr_weight * (1.0 - pd)
                + b * assignments.bass.front_lr_weight * (1.0 - pb)
                + h * assignments.harmonics.front_lr_weight * (1.0 - ph)
                + a * assignments.ambience.front_lr_weight * (1.0 - pa);

            ls[i] = v * assignments.voice.rear_lr_weight * (1.0 + pv)
                + d * assignments.drums.rear_lr_weight * (1.0 + pd)
                + b * assignments.bass.rear_lr_weight * (1.0 + pb)
                + h * assignments.harmonics.rear_lr_weight * (1.0 + ph)
                + a * assignments.ambience.rear_lr_weight * (1.0 + pa);

            rs[i] = v * assignments.voice.rear_lr_weight * (1.0 - pv)
                + d * assignments.drums.rear_lr_weight * (1.0 - pd)
                + b * assignments.bass.rear_lr_weight * (1.0 - pb)
                + h * assignments.harmonics.rear_lr_weight * (1.0 - ph)
                + a * assignments.ambience.rear_lr_weight * (1.0 - pa);

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
            pan: 0.0,
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

    #[test]
    fn deterministic_upmix_preserves_l_r_exactly() {
        let sr = 48000;
        let n = 4800;
        let left: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let right: Vec<f32> = (0..n).map(|i| (i as f32 * 0.013).sin() * 0.5).collect();
        let stage =
            FiveDotOneStage::deterministic_upmix(&left, &right, sr, &SpatialFirewall::default());

        // The core marketing/architectural promise: L/R must be bit-identical to input.
        assert_eq!(stage.l, left, "L channel must be untouched");
        assert_eq!(stage.r, right, "R channel must be untouched");
    }

    #[test]
    fn deterministic_upmix_centered_signal_goes_to_center() {
        let sr = 48000;
        let n = 4800;
        // Fully centered (mono-compatible) content: L == R.
        let mono: Vec<f32> = (0..n).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let stage =
            FiveDotOneStage::deterministic_upmix(&mono, &mono, sr, &SpatialFirewall::default());

        // Side should be ~zero (L==R means side=(L-R)*0.5=0), so Center should carry
        // most of the energy, Ls/Rs should be near-silent.
        let c_energy: f32 = stage.c.iter().map(|v| v * v).sum();
        let ls_energy: f32 = stage.ls.iter().map(|v| v * v).sum();
        assert!(
            c_energy > 0.0,
            "Center should carry energy from centered content"
        );
        assert!(
            ls_energy < c_energy * 0.01,
            "Ls should be near-silent for fully centered input, got ls_energy={} vs c_energy={}",
            ls_energy,
            c_energy
        );
    }

    #[test]
    fn deterministic_upmix_is_deterministic() {
        let sr = 48000;
        let left: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.02).sin()).collect();
        let right: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.021).sin()).collect();
        let fw = SpatialFirewall::default();
        let stage1 = FiveDotOneStage::deterministic_upmix(&left, &right, sr, &fw);
        let stage2 = FiveDotOneStage::deterministic_upmix(&left, &right, sr, &fw);
        assert_eq!(stage1.c, stage2.c);
        assert_eq!(stage1.ls, stage2.ls);
        assert_eq!(stage1.rs, stage2.rs);
    }

    #[test]
    fn deterministic_upmix_never_exceeds_input_true_peak_headroom() {
        // Worst-case stereo input: hard out-of-phase (L = -R), maximum Side energy,
        // at exactly the same peak level the stereo mastering chain already
        // guarantees (-1.0 dBTP ≈ linear 0.891, per today's Apple Digital Masters work).
        let sr = 48000;
        let n = 4800;
        let peak_linear = 10.0_f32.powf(-1.0 / 20.0); // ≈ 0.891
        let left: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 0.3).sin() * peak_linear)
            .collect();
        let right: Vec<f32> = left.iter().map(|&l| -l).collect(); // hard anti-phase: L = -R

        let stage =
            FiveDotOneStage::deterministic_upmix(&left, &right, sr, &SpatialFirewall::default());

        // Use the EXISTING TruePeakDetector (4x oversampled, the same one verified
        // earlier this session) on each derived channel, not just raw sample peak —
        // since the concern is specifically about intersample/filter-transient
        // overshoot, not steady-state magnitude.
        for (name, channel) in [
            ("C", &stage.c),
            ("LFE", &stage.lfe),
            ("Ls", &stage.ls),
            ("Rs", &stage.rs),
        ] {
            let mut detector = crate::limiter::true_peak::TruePeakDetector::new();
            let mut max_tp = 0.0_f32;
            for &s in channel {
                let tp = detector.process(s, s); // mono channel, feed same value to both detector inputs
                max_tp = max_tp.max(tp);
            }
            let max_tp_db = 20.0 * max_tp.max(1e-9).log10();
            println!(
                "[UPMIX-TP-CHECK] channel={} max_true_peak={:.4} ({:.2} dBTP)",
                name, max_tp, max_tp_db
            );
            assert!(max_tp_db <= -1.0 + 0.5, // small margin for measurement, not the actual safety budget
                "{} channel true peak {:.2} dBTP exceeds the -1.0 dBTP budget the stereo master was certified at — derived channel needs its own limiting stage before export",
                name, max_tp_db);
        }
    }
}
