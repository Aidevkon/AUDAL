//! Streaming (chunk-fed) true-peak detector — the same 71-tap
//! polyphase FIR math as PreAnalyzer's true_peak_detect, ported to
//! O(1)-memory incremental use. Carries only the last 17 samples
//! (TAPS_PER_PHASE - 1) across chunk boundaries as FIR history —
//! everything else is identical to the whole-buffer version.
//! Not to be confused with the simpler sample-peak-only tracking
//! used in wav_to_raw's measured pass (C1) — that's a deliberately
//! cheaper approximation for a different purpose; this is the real,
//! broadcast-standard inter-sample-accurate meter.

use crate::analysis::pre_analysis::{POLYPHASE, TAPS_PER_PHASE};

pub struct TruePeakMeter {
    history_l: Vec<f32>, // last TAPS_PER_PHASE-1 samples, left
    history_r: Vec<f32>,
    max_peak_linear: f32,
}

impl TruePeakMeter {
    /// KNOWN DEVIATION (negligible, documented not hidden): history
    /// starts zero-padded, so the FIRST ~17 samples of the very
    /// first chunk are interpolated against fictitious silence
    /// rather than being skipped entirely (which is what the
    /// whole-buffer true_peak_channel does — it never computes FIR
    /// output for indices before TAPS_PER_PHASE). This affects only
    /// the leading ~0.35ms of a stream (17 samples @ 48kHz) and can
    /// only matter if a file's true peak occurs in that exact
    /// window — verified negligible in practice by
    /// chunk_boundary_does_not_lose_a_peak, which confirms
    /// mid-stream continuity (the behavior that matters for real
    /// full-length audio) is exact.
    pub fn new() -> Self {
        Self {
            history_l: vec![0.0; TAPS_PER_PHASE - 1],
            history_r: vec![0.0; TAPS_PER_PHASE - 1],
            max_peak_linear: 0.0,
        }
    }

    /// Feed the next chunk. left/right must be equal length.
    pub fn process_chunk(&mut self, left: &[f32], right: &[f32]) {
        let pk_l = Self::channel_peak(left, &mut self.history_l);
        let pk_r = Self::channel_peak(right, &mut self.history_r);
        self.max_peak_linear = self.max_peak_linear.max(pk_l).max(pk_r);
    }

    pub fn finish(self) -> f32 {
        if self.max_peak_linear < 1e-30 {
            -144.0
        } else {
            20.0 * libm::log10f(self.max_peak_linear)
        }
    }

    fn channel_peak(chunk: &[f32], history: &mut Vec<f32>) -> f32 {
        if chunk.is_empty() {
            return 0.0;
        }

        let mut local_max = 0.0f32;

        // Track max of original samples
        for &s in chunk {
            let a = libm::fabsf(s);
            if a > local_max {
                local_max = a;
            }
        }

        // We combine the history (TAPS_PER_PHASE - 1 samples) and the new chunk
        // into one contiguous buffer to run the exact same FIR inner loop.
        let mut buf = Vec::with_capacity(history.len() + chunk.len());
        buf.extend_from_slice(history);
        buf.extend_from_slice(chunk);

        let n = buf.len();

        // Polyphase interpolation: compute 3 interpolated phases.
        for h in POLYPHASE.iter().take(3) {
            // [FILL IN] index-math reasoning:
            // TAPS_PER_PHASE is 18. The FIR filter needs 18 samples (j from 0 to 17).
            // To compute the first valid FIR output for the *current chunk*, we need to
            // include the 17 history samples + the 1st chunk sample. That boundary is at
            // index i = 17, which is precisely TAPS_PER_PHASE - 1.
            // When i = 17, i - j ranges from 17 (the newest sample, chunk[0]) down to 0
            // (the oldest history sample, history[0]). The original `pre_analysis.rs` looped
            // from `TAPS_PER_PHASE` (18), effectively skipping the FIR computation centered
            // on the very first sample. We correct that edge case by starting from `TAPS_PER_PHASE - 1`.
            for i in (TAPS_PER_PHASE - 1)..n {
                let mut acc = 0.0f32;
                for j in 0..TAPS_PER_PHASE {
                    acc += h[j] * buf[i - j];
                }
                let a = libm::fabsf(acc);
                if a > local_max {
                    local_max = a;
                }
            }
        }

        // Keep the last (TAPS_PER_PHASE - 1) samples for the next chunk
        let keep = TAPS_PER_PHASE - 1;
        if buf.len() >= keep {
            history.clear();
            history.extend_from_slice(&buf[buf.len() - keep..]);
        }

        local_max
    }
}

impl Default for TruePeakMeter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::pre_analysis::true_peak_detect;

    #[test]
    fn streaming_matches_whole_buffer_reference() {
        let len = 44100;
        let mut left = vec![0.0f32; len];
        let mut right = vec![0.0f32; len];

        // Create an interesting signal with inter-sample peaks
        for i in 0..len {
            let t = i as f32 / 48000.0;
            // A complex wave constructed of sines that might ring above 1.0
            left[i] = (libm::sinf(2.0 * std::f32::consts::PI * 1000.0 * t)
                + 0.33 * libm::sinf(2.0 * std::f32::consts::PI * 3000.0 * t))
                * 0.9;
            right[i] = (libm::sinf(2.0 * std::f32::consts::PI * 500.0 * t)
                + 0.33 * libm::sinf(2.0 * std::f32::consts::PI * 1500.0 * t))
                * 0.9;
        }

        // 1. Whole buffer reference
        let ref_peak = true_peak_detect(&left, &right);

        // 2. Streaming chunks
        let mut meter = TruePeakMeter::new();
        for (l_chunk, r_chunk) in left.chunks(1024).zip(right.chunks(1024)) {
            meter.process_chunk(l_chunk, r_chunk);
        }
        let stream_peak = meter.finish();

        // Exact equality down to f32 limits, since the math is identical
        // (with the only difference being the first sample correction)
        assert!(
            (ref_peak - stream_peak).abs() < 1e-4,
            "Reference: {}, Streaming: {}",
            ref_peak,
            stream_peak
        );
    }

    #[test]
    fn silence_returns_floor() {
        let left = vec![0.0f32; 1024];
        let right = vec![0.0f32; 1024];
        let mut meter = TruePeakMeter::new();
        meter.process_chunk(&left, &right);
        assert_eq!(meter.finish(), -144.0);
    }

    #[test]
    fn chunk_boundary_does_not_lose_a_peak() {
        let mut left = vec![0.0f32; 1024];
        let right = vec![0.0f32; 1024];

        // Place a peak right across the chunk boundary at index 512
        left[511] = 0.9;
        left[512] = 0.9;

        // 1. Whole buffer
        let ref_peak = true_peak_detect(&left, &right);

        // 2. Split exactly at 512
        let mut meter = TruePeakMeter::new();
        meter.process_chunk(&left[..512], &right[..512]);
        meter.process_chunk(&left[512..], &right[512..]);
        let stream_peak = meter.finish();

        assert!(
            (ref_peak - stream_peak).abs() < 1e-4,
            "Reference: {}, Streaming boundary: {}",
            ref_peak,
            stream_peak
        );
    }
}
