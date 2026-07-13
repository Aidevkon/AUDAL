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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegmentType {
    Speech,
    Music,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentBoundary {
    pub start_sec: f32,
    pub end_sec: f32,
    pub segment_type: SegmentType,
    pub avg_leaning: f32,
    /// Mean of RAW per-window confidence across the segment's windows —
    /// includes window-level noise (e.g. a mid-sentence pause dip).
    /// This reflects window-to-window measurement noise, NOT the
    /// smoothed segment-level certainty (which is why a clean, obvious
    /// speech segment can show a moderate avg_confidence like 0.53 —
    /// individual noisy windows pull the average down even though the
    /// smoothed leaning never wavered). Certificate consumers should
    /// treat this as "average per-window agreement," not "how sure are
    /// we this segment is correctly typed."
    pub avg_confidence: f32,
}

// PROVISIONAL — smoothing gain + crossing threshold validated on
// Flights 9-12 (2 clip pairs, both directions) but not corpus-tuned.
pub fn smooth_and_segment(decisions: &[(f32, ScoutDecision)]) -> Vec<SegmentBoundary> {
    if decisions.is_empty() {
        return vec![];
    }

    let mut segments = Vec::new();

    // Initial activation: we start at the first decision's raw leaning score.
    // Why? If a file starts purely with speech, we don't want to start at 0.5
    // and artificially ramp up, which could delay the "Speech" classification
    // if the confidence drops early. We snap to the actual initial state.
    let mut activation = decisions[0].1.leaning_score;
    let mut current_type = if activation >= 0.5 {
        SegmentType::Speech
    } else {
        SegmentType::Music
    };

    let mut current_start = decisions[0].0;

    // Accumulators for averages
    let mut sum_leaning = 0.0;
    let mut sum_confidence = 0.0;
    let mut window_count = 0;

    for &(t, ref dec) in decisions {
        let prev_activation = activation;
        activation += dec.confidence * (dec.leaning_score - activation);

        // We accumulate before the split check so the window that causes the crossing
        // is included in the previous segment? Wait. The window that pushes it over
        // the edge technically belongs to the *new* state. Let's accumulate it into
        // the *current* state before checking the boundary, then reset. This is fine.
        sum_leaning += dec.leaning_score;
        sum_confidence += dec.confidence;
        window_count += 1;

        let crossed_to_speech = prev_activation < 0.5 && activation >= 0.5;
        let crossed_to_music = prev_activation >= 0.5 && activation < 0.5;

        if crossed_to_speech || crossed_to_music {
            // Emit previous segment
            let avg_leaning = sum_leaning / window_count as f32;
            let avg_confidence = sum_confidence / window_count as f32;

            segments.push(SegmentBoundary {
                start_sec: current_start,
                end_sec: t, // The crossing time is the boundary
                segment_type: current_type,
                avg_leaning,
                avg_confidence,
            });

            // Reset for new segment
            current_type = if crossed_to_speech {
                SegmentType::Speech
            } else {
                SegmentType::Music
            };
            current_start = t;

            // Start the new segment clean
            sum_leaning = 0.0;
            sum_confidence = 0.0;
            window_count = 0;
        }
    }

    // Emit final segment
    // We use the timestamp of the last decision as the end_sec.
    // Why? `scout.rs` doesn't know the WINDOW_SECS constant (it's in `sp314-dsp`),
    // so using the last timestamp provided is the safest boundary without magic numbers.
    let last_t = decisions.last().unwrap().0;
    if window_count > 0 {
        segments.push(SegmentBoundary {
            start_sec: current_start,
            end_sec: last_t,
            segment_type: current_type,
            avg_leaning: sum_leaning / window_count as f32,
            avg_confidence: sum_confidence / window_count as f32,
        });
    }

    segments
}

// PROVISIONAL — dead-zone bounds + escalation trigger. Awaiting M2
// corpus calibration (measured confidence distributions across
// real content) to set the real threshold. REPLACE ON THE FLY:
// search "A7_PROVISIONAL" to find every value that needs updating
// once M2 data exists.
const A7_PROVISIONAL_DEAD_ZONE_LOW: f32 = 0.3;
const A7_PROVISIONAL_DEAD_ZONE_HIGH: f32 = 0.7;
const A7_PROVISIONAL_MIN_CONFIDENCE: f32 = 0.4; // below this, flag

pub fn needs_stem_escalation(boundary: &SegmentBoundary) -> bool {
    // A segment needs escalation if its leaning sits in the
    // ambiguous dead zone AND its confidence is low — i.e. the
    // mix-level Scout genuinely couldn't decide, not just noise
    // that smoothing already resolved.
    let in_dead_zone = boundary.avg_leaning > A7_PROVISIONAL_DEAD_ZONE_LOW
        && boundary.avg_leaning < A7_PROVISIONAL_DEAD_ZONE_HIGH;
    let low_confidence = boundary.avg_confidence < A7_PROVISIONAL_MIN_CONFIDENCE;
    in_dead_zone && low_confidence
}

pub fn flag_escalation_candidates(boundaries: &[SegmentBoundary]) -> Vec<usize> {
    // returns INDICES into the boundaries slice that need escalation
    boundaries
        .iter()
        .enumerate()
        .filter(|(_, b)| needs_stem_escalation(b))
        .map(|(i, _)| i)
        .collect()
}

#[derive(Clone, Debug)]
pub struct TimelineRouter {
    boundaries: Vec<SegmentBoundary>,
}

impl TimelineRouter {
    pub fn new(boundaries: Vec<SegmentBoundary>) -> Self {
        Self { boundaries }
    }

    pub fn get_segment_type_at(&self, timestamp_sec: f32) -> Option<SegmentType> {
        // Linear scan: Timeline Maps are extremely small (dozens of segments per file)
        // so O(N) is practically instantaneous and cache-friendly compared to binary search overhead.
        for b in &self.boundaries {
            if timestamp_sec >= b.start_sec && timestamp_sec < b.end_sec {
                return Some(b.segment_type);
            }
        }
        None
    }

    pub fn get_segment_at(&self, timestamp_sec: f32) -> Option<(usize, SegmentType)> {
        for (idx, b) in self.boundaries.iter().enumerate() {
            if timestamp_sec >= b.start_sec && timestamp_sec < b.end_sec {
                return Some((idx, b.segment_type));
            }
        }
        None
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

    // --- Part B: smooth_and_segment tests ---

    fn mk_dec(leaning_score: f32, confidence: f32) -> ScoutDecision {
        ScoutDecision {
            leaning_score,
            confidence,
            per_axis_normalized: [0.0; 4],
        }
    }

    #[test]
    fn test_stable_speech_no_boundary() {
        let stream = vec![
            (0.0, mk_dec(0.9, 0.9)),
            (1.0, mk_dec(0.85, 0.95)),
            (2.0, mk_dec(0.92, 0.9)),
        ];
        let segments = smooth_and_segment(&stream);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].segment_type, SegmentType::Speech);
        assert_eq!(segments[0].start_sec, 0.0);
        assert_eq!(segments[0].end_sec, 2.0);
    }

    #[test]
    fn test_stable_music_no_boundary() {
        let stream = vec![
            (0.0, mk_dec(0.1, 0.9)),
            (1.0, mk_dec(0.15, 0.95)),
            (2.0, mk_dec(0.05, 0.9)),
        ];
        let segments = smooth_and_segment(&stream);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].segment_type, SegmentType::Music);
        assert_eq!(segments[0].start_sec, 0.0);
        assert_eq!(segments[0].end_sec, 2.0);
    }

    #[test]
    fn test_clean_transition() {
        let stream = vec![
            (0.0, mk_dec(0.9, 0.9)),
            (1.0, mk_dec(0.9, 0.9)),
            (2.0, mk_dec(0.1, 0.9)), // Sharp transition to music
            (3.0, mk_dec(0.1, 0.9)),
        ];
        let segments = smooth_and_segment(&stream);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].segment_type, SegmentType::Speech);
        assert_eq!(segments[1].segment_type, SegmentType::Music);
        // The crossing happens at t=2.0 (activation drops < 0.5)
        assert_eq!(segments[0].end_sec, 2.0);
        assert_eq!(segments[1].start_sec, 2.0);
    }

    #[test]
    fn test_noisy_dip_no_false_boundary() {
        // ACTUAL numbers from Flight 10/12 (speech clip, t=2s dip)
        let stream = vec![
            (0.0, mk_dec(0.970, 0.953)),
            (1.0, mk_dec(0.832, 0.555)),
            (2.0, mk_dec(0.567, 0.341)), // The noisy dip!
            (3.0, mk_dec(0.605, 0.414)),
            (4.0, mk_dec(0.578, 0.362)),
            (5.0, mk_dec(0.939, 0.719)),
        ];
        let segments = smooth_and_segment(&stream);
        // Should ignore the dip and stay Speech
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].segment_type, SegmentType::Speech);
    }

    #[test]
    fn test_asymmetric_attack_release() {
        // Simulated IDM transition encoding the Flight 10/11 findings.
        // Music Entering (Fast Attack):
        let mut activation = 0.540; // The actual activation entering the blur zone in Flight 10
        let mut speech_to_music_cross = None;
        let attack_decisions = vec![
            mk_dec(0.150, 0.250), // Window 1 of blur
            mk_dec(0.150, 0.250), // Window 2 of blur
            mk_dec(0.000, 1.000), // Pure music
        ];
        for (i, dec) in attack_decisions.iter().enumerate() {
            let prev = activation;
            activation += dec.confidence * (dec.leaning_score - activation);
            if prev >= 0.5 && activation < 0.5 && speech_to_music_cross.is_none() {
                speech_to_music_cross = Some(i);
            }
        }

        // Music Leaving (Slow Release):
        activation = 0.198; // The actual activation entering the blur zone in Flight 11
        let mut music_to_speech_cross = None;
        let release_decisions = vec![
            mk_dec(0.626, 0.359), // Window 1 of blur (leaning Speech, but low conf!)
            mk_dec(0.704, 0.422), // Window 2 of blur
            mk_dec(0.970, 0.953), // Pure speech
        ];
        for (i, dec) in release_decisions.iter().enumerate() {
            let prev = activation;
            activation += dec.confidence * (dec.leaning_score - activation);
            if prev < 0.5 && activation >= 0.5 && music_to_speech_cross.is_none() {
                music_to_speech_cross = Some(i);
            }
        }

        // Attack should cross earlier (index 0) than Release (index 1)
        assert_eq!(speech_to_music_cross, Some(0)); // Crosses on the very first 0.150 window
        assert_eq!(music_to_speech_cross, Some(1)); // Has to wait for the second window to cross!
    }

    // --- Part C: Escalation Detection tests ---

    fn mk_boundary(avg_leaning: f32, avg_confidence: f32) -> SegmentBoundary {
        SegmentBoundary {
            start_sec: 0.0,
            end_sec: 1.0,
            segment_type: if avg_leaning >= 0.5 {
                SegmentType::Speech
            } else {
                SegmentType::Music
            },
            avg_leaning,
            avg_confidence,
        }
    }

    #[test]
    fn test_confident_speech_no_escalation() {
        let b = mk_boundary(0.95, 0.85);
        assert!(!needs_stem_escalation(&b));
    }

    #[test]
    fn test_confident_music_no_escalation() {
        let b = mk_boundary(0.05, 0.80);
        assert!(!needs_stem_escalation(&b));
    }

    #[test]
    fn test_ambiguous_low_confidence_escalates() {
        let b = mk_boundary(0.55, 0.35); // in dead zone (0.3-0.7) and conf < 0.4
        assert!(needs_stem_escalation(&b));
    }

    #[test]
    fn test_moderate_confidence_edge_speech_segment() {
        // ACTUAL numbers from Pass-1 integration test (clip_transition_st.wav):
        // Speech | 0.0s -> 15.0s | Avg Leaning: 0.716 | Avg Conf: 0.526
        let b = mk_boundary(0.716, 0.526);
        let escalation = needs_stem_escalation(&b);
        assert!(!escalation, "Real speech segment shouldn't escalate! It survived because leaning 0.716 > 0.7 AND conf 0.526 > 0.4");
    }

    #[test]
    fn test_flag_escalation_candidates_returns_correct_indices() {
        let boundaries = vec![
            mk_boundary(0.9, 0.9), // [0] Safe Speech
            mk_boundary(0.6, 0.2), // [1] Escalate! (dead zone, low conf)
            mk_boundary(0.1, 0.9), // [2] Safe Music
            mk_boundary(0.4, 0.3), // [3] Escalate! (dead zone, low conf)
        ];
        let flags = flag_escalation_candidates(&boundaries);
        assert_eq!(flags, vec![1, 3]);
    }
}
