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
    pub cv_ioi: f32,
    pub cepstral_flux: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoutDecision {
    pub leaning_score: f32, // 0.0 = Music, 1.0 = Speech
    pub confidence: f32,    // 0.0 = Unsure, 1.0 = Certain
}

pub fn mfcc_euclidean_distance(a: &[f32; 13], b: &[f32; 13]) -> f32 {
    let mut sq_diff = 0.0;
    for j in 1..13 {
        let diff = b[j] - a[j];
        sq_diff += diff * diff;
    }
    libm::sqrtf(sq_diff)
}

pub fn compute_cepstral_flux(mfccs: &[[f32; 13]]) -> f32 {
    if mfccs.len() < 2 {
        return 0.0;
    }
    let mut sum_dist = 0.0;
    for i in 1..mfccs.len() {
        sum_dist += mfcc_euclidean_distance(&mfccs[i - 1], &mfccs[i]);
    }
    sum_dist / (mfccs.len() - 1) as f32
}

// F-041 Evaluated Centroids
const MUSIC_CV: f32 = 0.4375;
#[allow(dead_code)]
const SPEECH_CV: f32 = 0.6855;
const CV_POOLED_STD: f32 = 0.1536;

const MUSIC_FLUX: f32 = 1.3387;
#[allow(dead_code)]
const SPEECH_FLUX: f32 = 1.7242;
const FLUX_POOLED_STD: f32 = 0.1738;

// Precomputed Z-space projection constants
const DELTA_CV: f32 = 1.61458;
const DELTA_FLUX: f32 = 2.21807;
const DELTA_SQ: f32 = 7.52673;

// Z-clamp: bounds each axis so one wild coordinate cannot dominate the
// dot product. Without this, a bimodal IOI distribution (silence + burst)
// can produce cv_ioi > 1.0, which is 5+ sigma past the music centroid —
// more extreme than speech ever is, yet the unclamped projection would
// read "maximally speech" with high confidence.
const Z_CLAMP: f32 = 3.0;

// Perpendicular distance penalty: addresses the known limitation of
// projection — a point unlike anything in training can project far along
// the centroid axis and would otherwise read confident. The penalty
// multiplies confidence by (1 - d_perp²/R²).clamp(0,1), so points far
// off-axis get their confidence suppressed while on-axis points are
// unaffected.
// R=3.0 chosen by measurement: speech d_perp p90=1.577, max=2.285,
// so 3.0 clears real speech while suppressing off-axis anomalies.
// R=2.0 costs 6 useful speech files; R=4.0 lets music_11 back in.
const PERP_R_SQ: f32 = 9.0;

pub fn compute_scout_decision(m: &ScoutMeasurements) -> ScoutDecision {
    if m.cv_ioi.is_nan()
        || m.cepstral_flux.is_nan()
        || m.cv_ioi.is_infinite()
        || m.cepstral_flux.is_infinite()
    {
        return ScoutDecision {
            leaning_score: 0.5,
            confidence: 0.0,
        };
    }

    // B: Clamp each z-score to [-Z_CLAMP, Z_CLAMP] before projecting.
    let p_cv = ((m.cv_ioi - MUSIC_CV) / CV_POOLED_STD).clamp(-Z_CLAMP, Z_CLAMP);
    let p_flux = ((m.cepstral_flux - MUSIC_FLUX) / FLUX_POOLED_STD).clamp(-Z_CLAMP, Z_CLAMP);

    // Project onto the delta line: t = ((p - c_music) · delta) / |delta|²
    let t = ((p_cv * DELTA_CV) + (p_flux * DELTA_FLUX)) / DELTA_SQ;
    let leaning_score = t.clamp(0.0, 1.0);

    // Symmetric confidence, peaking at endpoints and 0 at midpoint
    let conf_raw = (libm::fabsf(t - 0.5) * 2.0).clamp(0.0, 1.0);

    // C: Perpendicular distance penalty — suppress confidence for points
    // far from the centroid axis.
    let proj_cv = t * DELTA_CV;
    let proj_flux = t * DELTA_FLUX;
    let perp_cv = p_cv - proj_cv;
    let perp_flux = p_flux - proj_flux;
    let d_perp_sq = perp_cv * perp_cv + perp_flux * perp_flux;
    let penalty = (1.0 - d_perp_sq / PERP_R_SQ).clamp(0.0, 1.0);
    let confidence = conf_raw * penalty;

    ScoutDecision {
        leaning_score,
        confidence,
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
        // Was: encoding pure speech from Flight 4 using variance_a, etc.
        // Now: encoding the exact speech centroid on the new axes.
        let m = ScoutMeasurements {
            cv_ioi: 0.6855,
            cepstral_flux: 1.7242,
        };
        let decision = compute_scout_decision(&m);
        assert!((decision.leaning_score - 1.0).abs() < 0.01);
        assert!((decision.confidence - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_music_high_confidence() {
        // Was: encoding pure IDM music from Flight 4 using low variance/crest.
        // Now: encoding the exact music centroid on the new axes.
        let m = ScoutMeasurements {
            cv_ioi: 0.4375,
            cepstral_flux: 1.3387,
        };
        let decision = compute_scout_decision(&m);
        assert!((decision.leaning_score - 0.0).abs() < 0.01);
        assert!((decision.confidence - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hybrid_ad_low_confidence() {
        // Was: encoding a 2-2 axis split (Rhythm/Stereo = Music, MFCC/Crest = Speech) to force low confidence.
        // Now: encoding the exact midpoint between the centroids, where the projection t=0.5.
        let m = ScoutMeasurements {
            cv_ioi: 0.5615,
            cepstral_flux: 1.53145,
        };
        let decision = compute_scout_decision(&m);
        assert!((decision.leaning_score - 0.5).abs() < 0.01);
        assert!((decision.confidence - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_nan_input_low_confidence() {
        // Was: encoding mono music (correlation_d 1.0) with low variance to force a confidence collapse.
        // Now: encoding a NaN input (which occurs when onsets < 3) to verify fallback handles it securely.
        let m = ScoutMeasurements {
            cv_ioi: f32::NAN,
            cepstral_flux: 1.5,
        };
        let decision = compute_scout_decision(&m);
        assert_eq!(decision.leaning_score, 0.5);
        assert_eq!(decision.confidence, 0.0);
    }

    #[test]
    fn test_infinity_input_low_confidence() {
        // Was: encoding mono speech (correlation_d 1.0) with high variance to prove it didn't collapse.
        // Now: encoding an Infinity input to verify fallback handles it securely.
        let m = ScoutMeasurements {
            cv_ioi: 1.5,
            cepstral_flux: f32::INFINITY,
        };
        let decision = compute_scout_decision(&m);
        assert_eq!(decision.leaning_score, 0.5);
        assert_eq!(decision.confidence, 0.0);
    }

    #[test]
    fn test_compute_cepstral_flux_constant() {
        let mfccs = [[1.0; 13], [1.0; 13], [1.0; 13]];
        assert_eq!(compute_cepstral_flux(&mfccs), 0.0);
    }

    #[test]
    fn test_compute_cepstral_flux_alternating() {
        let mut v1 = [0.0; 13];
        let mut v2 = [0.0; 13];
        v1[1] = 3.0; // diff = 3
        v2[1] = 7.0; // diff = 4 -> sq = 16 -> sqrt = 4
                     // The distance is exactly 4.0
        let mfccs = [v1, v2, v1, v2];
        assert_eq!(compute_cepstral_flux(&mfccs), 4.0);
    }

    #[test]
    fn test_compute_cepstral_flux_short() {
        let mfccs = [[1.0; 13]];
        assert_eq!(compute_cepstral_flux(&mfccs), 0.0);
        let mfccs_empty: [[f32; 13]; 0] = [];
        assert_eq!(compute_cepstral_flux(&mfccs_empty), 0.0);
    }

    // --- Part B: smooth_and_segment tests ---

    fn mk_dec(leaning_score: f32, confidence: f32) -> ScoutDecision {
        ScoutDecision {
            leaning_score,
            confidence,
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
        let attack_decisions = [
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
        let release_decisions = [
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

    // --- Part D: Z-clamp + Perpendicular penalty oracle tests ---

    #[test]
    fn test_off_axis_anomaly_suppressed() {
        // The 21.0s anomaly from clip_transition_st.wav: bimodal IOI gives
        // cv_ioi=1.293 (z=5.57 unclamped), flux=1.279 (z=-0.34). Without
        // the clamp+penalty this reads leaning=1.0, confidence=1.0. With
        // B+C it must have leaning > 0.5 (still projects speech-ward) but
        // confidence < 0.1 (d_perp ≈ 2.63, penalty ≈ 0.23).
        let m = ScoutMeasurements {
            cv_ioi: 1.293,
            cepstral_flux: 1.279,
        };
        let d = compute_scout_decision(&m);
        assert!(
            d.leaning_score > 0.5,
            "Off-axis point should still lean speech-ward, got {}",
            d.leaning_score
        );
        assert!(
            d.confidence < 0.1,
            "Off-axis anomaly confidence must be suppressed below 0.1, got {}",
            d.confidence
        );
    }

    #[test]
    fn test_speech_centroid_unaffected_by_penalty() {
        // The speech centroid sits ON the axis: d_perp = 0, penalty = 1.0.
        // Confidence must remain 1.0.
        let m = ScoutMeasurements {
            cv_ioi: SPEECH_CV,
            cepstral_flux: SPEECH_FLUX,
        };
        let d = compute_scout_decision(&m);
        assert!(
            (d.leaning_score - 1.0).abs() < 0.01,
            "Speech centroid leaning should be ~1.0, got {}",
            d.leaning_score
        );
        assert!(
            (d.confidence - 1.0).abs() < 0.01,
            "Speech centroid confidence should be ~1.0 (on-axis, penalty=1), got {}",
            d.confidence
        );
    }

    #[test]
    fn test_music_centroid_unaffected_by_penalty() {
        // The music centroid sits ON the axis: d_perp = 0, penalty = 1.0.
        // Confidence must remain 1.0.
        let m = ScoutMeasurements {
            cv_ioi: MUSIC_CV,
            cepstral_flux: MUSIC_FLUX,
        };
        let d = compute_scout_decision(&m);
        assert!(
            (d.leaning_score - 0.0).abs() < 0.01,
            "Music centroid leaning should be ~0.0, got {}",
            d.leaning_score
        );
        assert!(
            (d.confidence - 1.0).abs() < 0.01,
            "Music centroid confidence should be ~1.0 (on-axis, penalty=1), got {}",
            d.confidence
        );
    }
}
