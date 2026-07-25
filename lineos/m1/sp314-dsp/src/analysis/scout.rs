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
use lineos_corpus::mfcc::MfccAnalyzer;
use lineos_corpus::scout::ScoutMeasurements;

pub struct SegmentScout {
    flux: SpectralFluxDetector,
    mfcc: MfccAnalyzer,
}

impl Default for SegmentScout {
    fn default() -> Self {
        Self::new()
    }
}

impl SegmentScout {
    pub fn new() -> Self {
        Self {
            flux: SpectralFluxDetector::new(),
            mfcc: MfccAnalyzer::new(),
        }
    }

    pub fn measure(
        &mut self,
        mono: &[f32],
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
    ) -> ScoutMeasurements {
        // Axis A: Variance of IOIs (Inter-Onset Intervals)
        let (_, onsets) = self.flux.detect(mono);
        let mut variance_a = 0.0;
        if onsets.len() > 1 {
            let mut iois = Vec::with_capacity(onsets.len() - 1);
            use crate::stft::HOP_SIZE;
            let hop_size = HOP_SIZE as f32;
            for i in 1..onsets.len() {
                let diff_frames = (onsets[i] - onsets[i - 1]) as f32;
                let ioi_ms = diff_frames * hop_size * 1000.0 / (sample_rate as f32);
                iois.push(ioi_ms);
            }
            let mean_ioi = iois.iter().sum::<f32>() / (iois.len() as f32);
            let mut sum_sq = 0.0;
            for &ioi in &iois {
                let diff = ioi - mean_ioi;
                sum_sq += diff * diff;
            }
            // Population variance
            variance_a = sum_sq / (iois.len() as f32);
        }

        // Axis B: MFCC Minimum Distance to known centroids
        let raw_mfcc = self.mfcc.compute(mono);

        use lineos_corpus::genre_centroids_generated::{
            ACOUSTIC_MFCC_MEAN, GLOBAL_MFCC_MEAN, GLOBAL_MFCC_STD, TECHNO_MFCC_MEAN,
        };

        let mut dist_techno_sq = 0.0;
        let mut dist_acoustic_sq = 0.0;
        for i in 0..13 {
            let z = (raw_mfcc[i] - GLOBAL_MFCC_MEAN[i]) / (GLOBAL_MFCC_STD[i] + 1e-8);
            let d_t = z - TECHNO_MFCC_MEAN[i];
            let d_a = z - ACOUSTIC_MFCC_MEAN[i];
            dist_techno_sq += d_t * d_t;
            dist_acoustic_sq += d_a * d_a;
        }
        let mfcc_dist_b = libm::sqrtf(dist_techno_sq).min(libm::sqrtf(dist_acoustic_sq));

        // Axis C: Crest Factor
        let crest_c = crate::analysis::dynamics::crest_factor_db(mono);

        // Axis D: Stereo Correlation
        let correlation_d = crate::analysis::stereo::stereo_correlation(left, right);

        ScoutMeasurements {
            variance_a,
            mfcc_dist_b,
            crest_c,
            correlation_d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn load_flight_clip_stereo(name: &str) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let clip_path = manifest_dir.join(format!("../../../flight_clips_stereo/{}", name));

        let mut reader = hound::WavReader::open(clip_path).expect("failed to open clip");
        let spec = reader.spec();
        let samples: Vec<i32> = reader.samples().map(|s| s.unwrap()).collect();

        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut mono = Vec::new();

        if spec.channels == 2 {
            for chunk in samples.chunks_exact(2) {
                let l = chunk[0] as f32 / 32768.0;
                let r = chunk[1] as f32 / 32768.0;
                left.push(l);
                right.push(r);
                mono.push((l + r) * 0.5);
            }
        } else {
            for s in samples {
                let m = s as f32 / 32768.0;
                left.push(m);
                right.push(m);
                mono.push(m);
            }
        }

        (mono, left, right)
    }

    #[test]
    fn test_segment_scout_on_flight_clips() {
        let mut scout = SegmentScout::new();

        // 1. SPEECH
        let (mono_s, left_s, right_s) = load_flight_clip_stereo("clip_speech_st.wav");
        let meas_speech = scout.measure(&mono_s, &left_s, &right_s, 48000);
        println!("Speech measurements: {:?}", meas_speech);
        // Assert speech ranges based on actual flight measurements
        assert!(
            meas_speech.variance_a > 1000.0,
            "Speech IOI variance should be very high"
        );
        assert!(
            meas_speech.mfcc_dist_b > 3.0,
            "Speech MFCC dist to music centroids should be high"
        );
        assert!(
            meas_speech.correlation_d > 0.9,
            "Speech is highly mono correlated"
        );

        // 2. MUSIC (IDM)
        let (mono_m, left_m, right_m) = load_flight_clip_stereo("clip_idm_st.wav");
        let meas_music = scout.measure(&mono_m, &left_m, &right_m, 48000);
        println!("IDM measurements: {:?}", meas_music);
        // Assert IDM ranges based on actual flight measurements
        assert!(
            meas_music.variance_a < 500.0,
            "IDM IOI variance should be very low"
        );
        assert!(
            meas_music.mfcc_dist_b < 2.5,
            "IDM MFCC dist should be relatively low"
        );
        // Assert the known original stereo correlation for IDM (~0.8451)
        assert!(
            (meas_music.correlation_d - 0.8451).abs() < 0.05,
            "IDM stereo correlation should be approx 0.8451, got {}",
            meas_music.correlation_d
        );
    }

    #[test]
    fn test_segment_scout_full_pipeline() {
        let mut scout = SegmentScout::new();

        // 1. SPEECH
        let (mono_s, left_s, right_s) = load_flight_clip_stereo("clip_speech_st.wav");
        let meas_speech = scout.measure(&mono_s, &left_s, &right_s, 48000);
        let decision_speech = lineos_corpus::scout::compute_scout_decision(&meas_speech);
        println!("SPEECH MEAS -> {:?}", meas_speech);
        println!(
            "SPEECH -> Leaning: {:.3}, Confidence: {:.3}",
            decision_speech.leaning_score, decision_speech.confidence
        );
        assert!(
            decision_speech.leaning_score > 0.8,
            "Speech should have high leaning score"
        );
        assert!(
            decision_speech.confidence >= 0.75,
            "Speech confidence capped at 0.75 due to mono abstain"
        );

        // 2. IDM (MUSIC)
        let (mono_i, left_i, right_i) = load_flight_clip_stereo("clip_idm_st.wav");
        let meas_idm = scout.measure(&mono_i, &left_i, &right_i, 48000);
        let decision_idm = lineos_corpus::scout::compute_scout_decision(&meas_idm);
        println!("IDM MEAS -> {:?}", meas_idm);
        println!(
            "IDM    -> Leaning: {:.3}, Confidence: {:.3}",
            decision_idm.leaning_score, decision_idm.confidence
        );
        assert!(
            decision_idm.leaning_score < 0.2,
            "IDM should have low leaning score"
        );
        assert!(
            decision_idm.confidence > 0.8,
            "IDM should have high confidence"
        );

        // 3. ACOUSTIC (MUSIC)
        let (mono_a, left_a, right_a) = load_flight_clip_stereo("clip_acoustic_st.wav");
        let meas_acoustic = scout.measure(&mono_a, &left_a, &right_a, 48000);
        let decision_acoustic = lineos_corpus::scout::compute_scout_decision(&meas_acoustic);
        println!("ACOUSTIC MEAS -> {:?}", meas_acoustic);
        println!(
            "ACOUSTIC -> Leaning: {:.3}, Confidence: {:.3}",
            decision_acoustic.leaning_score, decision_acoustic.confidence
        );
        assert!(
            decision_acoustic.leaning_score < 0.4,
            "Acoustic should be music-leaning (low score)"
        );
        // Acoustic may not be as confident as pure IDM due to acoustic crest/variance
        assert!(
            decision_acoustic.confidence > 0.4,
            "Acoustic should have reasonable confidence"
        );
    }
}
