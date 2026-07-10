//! Scout Decision Engine (Phase 8)
//!
//! Provides the interpretation layer for the SegmentScout.
//! Takes pure DSP measurements from sp314-dsp and maps them
//! into a `ScoutDecision` (speech vs. music leaning, plus confidence)
//! based on predefined, provisional boundaries.
//!
//! SOURCE AGNOSTIC: The Scout NEVER assumes what it measures.
//! Today it measures the raw stereo Mix (M1).
//! Tomorrow (via A7 architecture) it will measure individual
//! separated stems. The logic here is purely mathematical
//! and agnostic to its input source.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoutMeasurements {
    pub variance_a: f32,
    pub mfcc_dist_b: f32,
    pub crest_c: f32,
    pub correlation_d: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoutDecision {
    pub leaning_score: f32, // 0.0 = Music, 1.0 = Speech
    pub confidence: f32,    // 0.0 = Unsure (axes split), 1.0 = Certain (axes agree)
    pub per_axis_normalized: [f32; 4],
}

// PROVISIONAL — hand-tuned on 4 flight clips (Flights 4-8),
// NOT corpus-calibrated. Awaiting M2 corpus for real tuning.
const WEIGHT_A: f32 = 0.40;
const WEIGHT_B: f32 = 0.15;
const WEIGHT_C: f32 = 0.10;
const WEIGHT_D: f32 = 0.35;

pub fn compute_scout_decision(m: &ScoutMeasurements) -> ScoutDecision {
    let norm_a = ((m.variance_a - 500.0) / 2500.0).clamp(0.0, 1.0);
    let norm_b = ((m.mfcc_dist_b - 3.2) / 1.0).clamp(0.0, 1.0);
    let norm_c = ((m.crest_c - 14.0) / 4.0).clamp(0.0, 1.0);
    let norm_d = ((m.correlation_d - 0.85) / 0.13).clamp(0.0, 1.0);

    // Mono input needs no special handling: when correlation reads ~1.0
    // on genuine mono music, Axis D disagrees with A/B/C, the
    // axis-variance confidence collapses automatically (mono music ->
    // low confidence, correctly flagged uncertain), while mono speech
    // (all axes agree) stays high-confidence. The disagreement metric IS
    // the mono handler.
    let per_axis_normalized = [norm_a, norm_b, norm_c, norm_d];

    let leaning_score =
        (norm_a * WEIGHT_A) + (norm_b * WEIGHT_B) + (norm_c * WEIGHT_C) + (norm_d * WEIGHT_D);

    // Compute population variance of the 4 normalized values.
    let mean = (norm_a + norm_b + norm_c + norm_d) / 4.0;
    let mut sum_sq_diff = 0.0;
    for &val in &per_axis_normalized {
        let diff = val - mean;
        sum_sq_diff += diff * diff;
    }
    let variance = sum_sq_diff / 4.0;

    let max_variance = 0.25;
    let normalized_variance = (variance / max_variance).clamp(0.0, 1.0);
    let confidence = 1.0 - normalized_variance;

    ScoutDecision {
        leaning_score,
        confidence,
        per_axis_normalized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speech_high_confidence() {
        // ACTUAL numbers from Flight 4: pure speech (ishaiaTEST.mp3)
        let m = ScoutMeasurements {
            variance_a: 7591.0,
            mfcc_dist_b: 4.17,
            crest_c: 16.62,
            correlation_d: 0.998,
        };
        let decision = compute_scout_decision(&m);
        assert!(decision.leaning_score > 0.8);
        assert!(decision.confidence > 0.8);
    }

    #[test]
    fn test_music_high_confidence() {
        // ACTUAL numbers from Flight 4: pure IDM (Databend.mp3)
        let m = ScoutMeasurements {
            variance_a: 142.0,
            mfcc_dist_b: 1.89,
            crest_c: 12.81,
            correlation_d: 0.8451,
        };
        let decision = compute_scout_decision(&m);
        assert!(decision.leaning_score < 0.2);
        assert!(decision.confidence > 0.8);
    }

    #[test]
    fn test_hybrid_ad_low_confidence() {
        // PROVISIONAL/SIMULATED numbers encoding the A8 phenomenon.
        // Flight 8 was aborted before final AD measurements were pulled,
        // so these numbers are carefully chosen to exactly represent the
        // A8 2-2 axis split (Rhythm/Stereo = Music, MFCC/Crest = Speech)
        // to prove the confidence collapses to 0.0 under max variance.
        let m = ScoutMeasurements {
            variance_a: 150.0,   // Low -> norm 0.0 (like Music)
            correlation_d: 0.70, // Low -> norm 0.0 (like Music)
            mfcc_dist_b: 4.5,    // High -> norm 1.0 (like Speech)
            crest_c: 18.0,       // High -> norm 1.0 (like Speech)
        };

        let decision = compute_scout_decision(&m);

        // The A8 property: a split 2-2 vote MUST yield low confidence,
        // regardless of where the exact leaning_score lands.
        assert!(
            decision.confidence < 0.05,
            "Hybrid ad (A8) with split axes must yield near-zero confidence, got {}",
            decision.confidence
        );
    }

    #[test]
    fn test_mono_music_low_confidence() {
        // Mono music (correlation 1.0) with low variance, low dist, low crest.
        let m = ScoutMeasurements {
            variance_a: 142.0,
            mfcc_dist_b: 1.89,
            crest_c: 12.81,
            correlation_d: 1.0,
        };
        let decision = compute_scout_decision(&m);

        // Must stay music-leaning (Axis D pushes it up, but A/B/C pull it down)
        assert!(
            decision.leaning_score < 0.5,
            "Mono music should lean music, got {}",
            decision.leaning_score
        );
        // Confidence collapses because D (1.0) disagrees with A/B/C (~0.0)
        assert!(
            decision.confidence < 0.4,
            "Confidence should collapse for mono music, got {}",
            decision.confidence
        );
    }

    #[test]
    fn test_mono_speech_still_works() {
        // Mono speech (correlation 1.0) with high variance, high dist, high crest.
        let m = ScoutMeasurements {
            variance_a: 7591.0,
            mfcc_dist_b: 4.17,
            crest_c: 16.62,
            correlation_d: 1.0,
        };
        let decision = compute_scout_decision(&m);

        // Must stay speech-leaning
        assert!(
            decision.leaning_score > 0.8,
            "Mono speech should remain high leaning"
        );
        // Confidence must be high (NOT capped) because all 4 axes agree
        assert!(
            decision.confidence > 0.8,
            "Mono speech confidence should be high, got {}",
            decision.confidence
        );
    }
}
