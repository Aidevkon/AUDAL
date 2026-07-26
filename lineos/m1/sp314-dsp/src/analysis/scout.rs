//! Segment Scout Measurement
//!
//! Provides the measurement half of the SegmentScout pipeline.
//! Runs the 4 DSP axes on a given window and returns raw `ScoutMeasurements`.
//!
//! SOURCE AGNOSTIC: The Scout NEVER assumes what it measures.
//! Today it measures the raw stereo Mix (M1).
//! Tomorrow (via A7 architecture) it will measure individual
//! separated stems. The logic here is purely mathematical
//! and agnostic to its input source.

use crate::stft::SpectralFluxDetector;
use lineos_corpus::scout::ScoutMeasurements;

pub struct SegmentScout {
    flux: SpectralFluxDetector,
}

impl Default for SegmentScout {
    fn default() -> Self {
        Self::new()
    }
}

impl SegmentScout {
    pub fn new() -> Self {
        Self {
            flux: SpectralFluxDetector::new(0.20_f32),
        }
    }

    pub fn measure(
        &mut self,
        mono: &[f32],
        cepstral_flux: f32,
        sample_rate: u32,
    ) -> ScoutMeasurements {
        let mut cv_ioi = f32::NAN;
        let (_, onsets) = self.flux.detect(mono);

        // Guard: Need at least 3 onsets for 2 IOIs to calculate variance.
        if onsets.len() >= 3 {
            use crate::stft::HOP_SIZE;
            let hop_size = HOP_SIZE as f32;
            let mut iois = Vec::with_capacity(onsets.len() - 1);

            for i in 1..onsets.len() {
                let diff_frames = (onsets[i] - onsets[i - 1]) as f32;
                let ioi_ms = diff_frames * hop_size * 1000.0 / (sample_rate as f32);
                iois.push(ioi_ms);
            }

            let mean_ioi = iois.iter().sum::<f32>() / (iois.len() as f32);

            // Guard against div-by-zero
            if mean_ioi > 1e-8 {
                let mut sum_sq = 0.0;
                for &ioi in &iois {
                    let diff = ioi - mean_ioi;
                    sum_sq += diff * diff;
                }
                // Population variance
                let variance = sum_sq / (iois.len() as f32);
                cv_ioi = libm::sqrtf(variance) / mean_ioi;
            }
        }

        ScoutMeasurements {
            cv_ioi,
            cepstral_flux,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_corpus::scout::compute_scout_decision;

    #[test]
    fn test_measure_nan_fallback() {
        // Was: test_segment_scout_on_flight_clips (loading real files to check old variance_a).
        // Now: Verifying that a buffer with fewer than 3 onsets returns NaN and yields an ambiguous decision.
        let mut scout = SegmentScout::new();
        let mono = vec![0.0; 48000]; // silence = 0 onsets
        let meas = scout.measure(&mono, 1.5, 48000);
        assert!(meas.cv_ioi.is_nan());

        let decision = compute_scout_decision(&meas);
        assert_eq!(decision.leaning_score, 0.5);
        assert_eq!(decision.confidence, 0.0);
    }

    #[test]
    fn test_periodic_click_train() {
        // Clicks every 6144 samples (12 STFT frames of 512).
        // All gaps are multiples of HOP_SIZE and >= FLUX_MIN_DISTANCE (10 frames).
        // Expected onset frames: 0, 12, 24, 36, 48, 60, 72, 84
        // Expected IOIs (frames): all 12  ->  CV = 0.0
        let mut scout = SegmentScout::new();
        let mut mono = vec![0.0; 48000];

        let positions = [0, 6144, 12288, 18432, 24576, 30720, 36864, 43008];
        for &p in &positions {
            mono[p] = 1.0;
            if p + 1 < 48000 {
                mono[p + 1] = 1.0;
            }
        }

        let meas = scout.measure(&mono, 1.5, 48000);
        println!("Periodic CV: {}", meas.cv_ioi);

        // Re-run to print onsets (measure consumed the detector state)
        let mut scout2 = SegmentScout::new();
        let (_, onsets) = scout2.flux.detect(&mono);
        println!("Periodic onsets ({} total): {:?}", onsets.len(), onsets);

        assert!(!meas.cv_ioi.is_nan(), "CV should not be NaN");
        assert!(
            meas.cv_ioi < 0.05,
            "Periodic train should have low CV, got {}",
            meas.cv_ioi
        );
    }

    #[test]
    fn test_irregular_click_train() {
        // Gaps of 12, 20, 14, 30, 16 frames = 6144, 10240, 7168, 15360, 8192 samples.
        // All gaps are multiples of HOP_SIZE and >= FLUX_MIN_DISTANCE.
        // Positions: 0, 6144, 16384, 23552, 38912, 47104
        // Expected IOIs (frames): 12, 20, 14, 30, 16  ->  CV ≈ 0.346
        let mut scout = SegmentScout::new();
        let mut mono = vec![0.0; 48000];

        let positions = [0usize, 6144, 16384, 23552, 38912, 47104];
        for &p in &positions {
            mono[p] = 1.0;
            if p + 1 < 48000 {
                mono[p + 1] = 1.0;
            }
        }

        let meas = scout.measure(&mono, 1.5, 48000);
        println!("Irregular CV: {}", meas.cv_ioi);

        // Re-run to print onsets
        let mut scout2 = SegmentScout::new();
        let (_, onsets) = scout2.flux.detect(&mono);
        println!("Irregular onsets ({} total): {:?}", onsets.len(), onsets);

        assert!(!meas.cv_ioi.is_nan(), "CV should not be NaN");
        assert!(
            meas.cv_ioi > 0.25,
            "Irregular train should have higher CV, got {}",
            meas.cv_ioi
        );
    }
}
