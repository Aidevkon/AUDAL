use crate::analysis::vad_sensors::{
    frame_spectral_flatness, mid_side_ratio, MfccSensor, RmsDeltaSensor, TransientSensor,
    FRAME_SAMPLES,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadFeatures {
    pub spectral_flatness: f32,
    pub rms_db: f32,
    pub rms_delta_db: f32,
    pub mid_side_ratio: f32,
    pub transient_density: f32,
    pub mfcc: [f32; 13],
}

pub struct VadFeatureExtractor {
    rms_delta: RmsDeltaSensor,
    transient: TransientSensor,
    mfcc: MfccSensor,
    pub leftover_mono: Vec<f32>,
    pub leftover_left: Vec<f32>,
    pub leftover_right: Vec<f32>,
}

impl Default for VadFeatureExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl VadFeatureExtractor {
    pub fn new() -> Self {
        Self {
            rms_delta: RmsDeltaSensor::new(),
            transient: TransientSensor::new(),
            mfcc: MfccSensor::new(),
            leftover_mono: Vec::with_capacity(FRAME_SAMPLES * 10),
            leftover_left: Vec::with_capacity(FRAME_SAMPLES * 10),
            leftover_right: Vec::with_capacity(FRAME_SAMPLES * 10),
        }
    }

    pub fn process_chunk(&mut self, mono: &[f32], left: &[f32], right: &[f32]) -> Vec<VadFeatures> {
        debug_assert_eq!(mono.len(), left.len());
        debug_assert_eq!(mono.len(), right.len());

        // Defensively bind to the minimum length to prevent any indexing panics
        // if an alignment mistake propagates to this call.
        let n = mono.len().min(left.len()).min(right.len());
        let safe_mono = &mono[..n];
        let safe_left = &left[..n];
        let safe_right = &right[..n];

        self.leftover_mono.extend_from_slice(safe_mono);
        self.leftover_left.extend_from_slice(safe_left);
        self.leftover_right.extend_from_slice(safe_right);

        let mut features = Vec::with_capacity(self.leftover_mono.len() / FRAME_SAMPLES);

        let mut consumed = 0usize;
        while consumed + FRAME_SAMPLES <= self.leftover_mono.len() {
            let end = consumed + FRAME_SAMPLES;

            // take the three frame slices by index (no mutation yet)
            let f_mono = &self.leftover_mono[consumed..end];
            let f_left = &self.leftover_left[consumed..end];
            let f_right = &self.leftover_right[consumed..end];

            let spectral_flatness = frame_spectral_flatness(f_mono);
            let (rms_db, rms_delta_db) = self.rms_delta.process(f_mono);
            let ms_ratio = mid_side_ratio(f_left, f_right);
            let transient_density = self.transient.process(f_mono);
            let mfcc = self.mfcc.process(f_mono);

            features.push(VadFeatures {
                spectral_flatness,
                rms_db,
                rms_delta_db,
                mid_side_ratio: ms_ratio,
                transient_density,
                mfcc,
            });

            consumed = end;
        }

        if consumed > 0 {
            self.leftover_mono.drain(..consumed);
            self.leftover_left.drain(..consumed);
            self.leftover_right.drain(..consumed);
        }

        features
    }
}

// ── Oracles (Tests) ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_frame_count_and_leftover() {
        let mut extractor = VadFeatureExtractor::new();
        let mono = vec![0.0; 4800];
        let left = vec![0.0; 4800];
        let right = vec![0.0; 4800];

        // 4800 in -> exactly 10 frames
        let f1 = extractor.process_chunk(&mono, &left, &right);
        assert_eq!(f1.len(), 10);
        assert_eq!(extractor.leftover_mono.len(), 0);

        // 5000 in -> exactly 10 frames, 200 leftover
        let mono5k = vec![0.0; 5000];
        let left5k = vec![0.0; 5000];
        let right5k = vec![0.0; 5000];
        let f2 = extractor.process_chunk(&mono5k, &left5k, &right5k);
        assert_eq!(f2.len(), 10);
        assert_eq!(extractor.leftover_mono.len(), 200);

        // 460 in -> 200 + 460 = 660. 660 / 480 = 1 frame. leftover = 180
        let mono460 = vec![0.0; 460];
        let left460 = vec![0.0; 460];
        let right460 = vec![0.0; 460];
        let f3 = extractor.process_chunk(&mono460, &left460, &right460);
        assert_eq!(f3.len(), 1);
        assert_eq!(extractor.leftover_mono.len(), 180);
    }

    #[test]
    fn oracle_alignment() {
        use crate::analysis::vad_sensors::MID_SIDE_SIDE_CAP;

        let mut extractor = VadFeatureExtractor::new();

        let mut mono = vec![0.0; 4800];
        let mut left = vec![0.0; 4800];
        let mut right = vec![0.0; 4800];

        // Mono silent [0..2400], tone [2400..4800]
        // Left steady tone
        // Right tone [0..2400], anti-phase (inverted) [2400..4800]
        for i in 0..4800 {
            let tone = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0) * 0.5;
            left[i] = tone;
            if i < 2400 {
                mono[i] = 0.0;
                right[i] = tone;
            } else {
                mono[i] = tone;
                right[i] = -tone;
            }
        }

        let features = extractor.process_chunk(&mono, &left, &right);
        assert_eq!(features.len(), 10);

        // The spike in mono and flip in right happen exactly at sample 2400.
        // Sample 2400 is the start of frame 5 (0-indexed).
        let f_prev = &features[4];
        let f_spike = &features[5];

        // Assert they were normal before
        assert!(
            f_prev.rms_delta_db < 1.0,
            "rms_delta_db should be near 0 at frame 4"
        );
        assert_eq!(
            f_prev.mid_side_ratio, 0.0,
            "mid_side_ratio should be 0 at frame 4"
        );

        // Assert they BOTH spike simultaneously at frame 5
        assert!(
            f_spike.rms_delta_db > 50.0,
            "rms_delta_db must spike at frame {}; ms_ratio spiked at frame {}",
            5,
            5
        );
        assert_eq!(
            f_spike.mid_side_ratio, MID_SIDE_SIDE_CAP,
            "mid_side_ratio must hit CAP at frame {}; rms_delta spiked at frame {}",
            5, 5
        );
    }

    #[test]
    fn oracle_split_invariance() {
        // LCG for reproducible deterministic chaotic signal
        let mut seed = 12345u32;
        let mut mono = vec![0.0; 10000];
        let mut left = vec![0.0; 10000];
        let mut right = vec![0.0; 10000];

        // Generate distinct pseudo-noise for all three channels
        for i in 0..10000 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            mono[i] = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            left[i] = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            right[i] = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
        }

        let mut ext_a = VadFeatureExtractor::new();
        let features_a = ext_a.process_chunk(&mono, &left, &right);

        let mut ext_b = VadFeatureExtractor::new();
        let mut features_b = Vec::new();
        // 3 arbitrary splits
        features_b.extend(ext_b.process_chunk(&mono[0..3333], &left[0..3333], &right[0..3333]));
        features_b.extend(ext_b.process_chunk(
            &mono[3333..6666],
            &left[3333..6666],
            &right[3333..6666],
        ));
        features_b.extend(ext_b.process_chunk(
            &mono[6666..10000],
            &left[6666..10000],
            &right[6666..10000],
        ));

        assert_eq!(features_a.len(), features_b.len());
        // Perfect structural equality across all frames
        for (i, (fa, fb)) in features_a.iter().zip(features_b.iter()).enumerate() {
            assert_eq!(
                fa, fb,
                "Features differ at frame {} (struct: {:?} vs {:?})",
                i, fa, fb
            );
        }
    }
}
