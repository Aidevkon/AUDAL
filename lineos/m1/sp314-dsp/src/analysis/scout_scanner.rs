use crate::analysis::scout::SegmentScout;
use lineos_corpus::scout::{compute_scout_decision, ScoutDecision};

// PROVISIONAL: validated via Flights 9-12 to provide stable rhythm/timbre
// tracking and robust confidence-collapse at boundaries.
const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;

/// Orchestrates a rolling SegmentScout scan across an audio buffer.
///
/// SOURCE-AGNOSTIC: This scans whatever buffer it is given. Today, that is
/// the stereo mix. Tomorrow, it could be an isolated stem (via A7 routing).
/// The scanner makes no assumptions about the content, only that it is a
/// coherent signal to be diagnosed.
pub fn scan_file(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> Vec<(f32, ScoutDecision)> {
    let mut decisions = Vec::new();
    
    // Safety check
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return decisions;
    }

    // Pre-compute mono for the entire buffer.
    // Why? With a 5s window and 1s hop, windows overlap by 4s.
    // If we computed mono per-slice, we'd do the math 5x for most samples.
    // Computing it once upfront is O(N) memory but 5x faster.
    let mono_full: Vec<f32> = left
        .iter()
        .zip(right.iter())
        .map(|(&l, &r)| (l + r) * 0.5)
        .collect();

    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;

    // A single SegmentScout instance is reused across all windows.
    // This prevents re-allocating STFT engine buffers per window.
    let mut scout = SegmentScout::new();
    let mut start = 0;

    // We drop the final partial window. A partial window (e.g. 1.2s instead of 5.0s)
    // skews the variance and density axes, producing unreliable measurements.
    while start + win_samples <= mono_full.len() {
        let end = start + win_samples;
        let mono_slice = &mono_full[start..end];
        let left_slice = &left[start..end];
        let right_slice = &right[start..end];

        let start_sec = start as f32 / sample_rate as f32;

        let meas = scout.measure(mono_slice, left_slice, right_slice, sample_rate);
        let decision = compute_scout_decision(&meas);

        decisions.push((start_sec, decision));
        start += hop_samples;
    }

    decisions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn load_flight_clip_stereo(path: &str) -> (Vec<f32>, Vec<f32>, Vec<f32>, u32) {
        let mut reader = hound::WavReader::open(path).expect("failed to open clip");
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

        (mono, left, right, spec.sample_rate)
    }

    #[test]
    fn test_scan_file_sanity() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let clip_path = manifest_dir
            .join("../../../flight_clips_stereo/clip_transition_st.wav")
            .to_string_lossy()
            .to_string();

        let (_mono, left, right, sample_rate) = load_flight_clip_stereo(&clip_path);
        
        let decisions = scan_file(&left, &right, sample_rate);
        
        // Duration is ~35 seconds. 
        // 35 - 5 (window) = 30 seconds of slidable area at 1s hops = ~31 windows.
        assert!(!decisions.is_empty(), "Scanner returned empty decisions");
        println!("Number of windows: {}", decisions.len());

        let mut prev_time = -1.0;
        for (i, &(t, ref decision)) in decisions.iter().enumerate() {
            println!("Time {:05.1}s | Leaning: {:.3} | Conf: {:.3}", t, decision.leaning_score, decision.confidence);
            // Check monotonicity and hop spacing
            if i > 0 {
                assert!((t - prev_time - HOP_SECS).abs() < 0.001, "Timestamps not spaced by HOP_SECS");
            }
            prev_time = t;
            
            // Basic leaning checks
            // Speech region is ~0s to 17s. We check windows that end before 16s.
            if t + WINDOW_SECS < 16.0 {
                assert!(decision.leaning_score > 0.5, "Speech region leaning too low at {:.1}s", t);
            }
            // IDM region is ~17s to 35s. We check windows that start after 18s.
            if t > 18.0 {
                assert!(decision.leaning_score < 0.3, "Music region leaning too high at {:.1}s", t);
            }
        }
        
        println!("Sanity test passed. Verified monotonic timestamps and region leanings.");
    }
}
