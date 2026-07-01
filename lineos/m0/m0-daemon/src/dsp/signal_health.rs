//! SignalHealthMonitor — streaming input
//! validation + dead-air tracking.
//!
//! Replaces the decode_node full-file RMS check
//! with a streaming, two-tier version that never
//! holds the whole file in RAM:
//!
//!   Tier 1 (early abort): after the first ~30s,
//!     if the signal is digital silence
//!     (< -100 dBFS) the file is dead/corrupt —
//!     fail fast before wasting an hour of DSP.
//!
//!   Tier 2 (full-file parity): at EOF, the total
//!     RMS reproduces decode_node's original check
//!     (< -60 dBFS = silence error; normalization
//!     gain > 30 dB = 32x guard). A file that had
//!     30s of speech then two hours of dead mic
//!     still fails at EOF, exactly like today.
//!
//! Dead-air gaps mid-episode are NOT fatal — they
//! are recorded as timeline events for the AI
//! coach ("noticed a gap at 45:10 — intentional?").
//!
//! Accumulation is f64 (matches decode_node's f64
//! path for verdict parity) with libm math for
//! cross-platform determinism.

/// A stretch of near-silence in the middle of an
/// episode. Non-fatal — surfaced as metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct DeadAirEvent {
    pub start_sec: f32,
    pub duration_sec: f32,
}

/// Digital-silence floor for Tier 1 early abort.
const DIGITAL_SILENCE_DBFS: f32 = -100.0;
/// Silence floor for Tier 2 (matches decode_node).
const SILENCE_DBFS: f32 = -60.0;
/// Max normalization gain before the 32x guard
/// trips (matches decode_node).
const MAX_GAIN_DB: f32 = 30.0;
/// Loudness target used by the gain guard
/// (matches decode_node: -14 - rough_lufs).
const GAIN_TARGET_LUFS: f32 = -14.0;
/// Window over which the early-abort verdict is
/// formed.
const EARLY_WINDOW_SECS: f64 = 30.0;
/// A 1-second window whose RMS is below this is
/// counted toward a dead-air run.
const DEAD_AIR_WINDOW_DBFS: f32 = -60.0;
/// A dead-air run must last at least this long to
/// be reported (avoids flagging natural pauses).
const DEAD_AIR_MIN_SECS: f32 = 3.0;

pub struct SignalHealthMonitor {
    sample_rate: u32,

    // Whole-stream accumulation (Tier 2).
    total_sum_sq: f64,
    total_frames: u64,

    // First-EARLY_WINDOW_SECS accumulation (Tier 1).
    early_sum_sq: f64,
    early_frames: u64,

    // 1-second rolling window for dead-air.
    window_sum_sq: f64,
    window_frames: u64,
    elapsed_secs: f64,

    // Current dead-air run, if any.
    dead_air_run_start: Option<f32>,
    dead_air: Vec<DeadAirEvent>,
}

impl SignalHealthMonitor {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            total_sum_sq: 0.0,
            total_frames: 0,
            early_sum_sq: 0.0,
            early_frames: 0,
            window_sum_sq: 0.0,
            window_frames: 0,
            elapsed_secs: 0.0,
            dead_air_run_start: None,
            dead_air: Vec::new(),
        }
    }

    /// Observe an interleaved stereo chunk (L,R,L,R
    /// ...). Frame count = samples / 2.
    pub fn observe(&mut self, interleaved: &[f32]) {
        let frames = interleaved.len() / 2;
        if frames == 0 {
            return;
        }
        let early_cap = (EARLY_WINDOW_SECS * self.sample_rate as f64) as u64;
        let window_cap = self.sample_rate as u64; // 1s

        for f in 0..frames {
            let l = interleaved[f * 2] as f64;
            let r = interleaved[f * 2 + 1] as f64;
            // Mean of the two channels' squares —
            // a mono-equivalent energy per frame.
            let sq = (l * l + r * r) * 0.5;

            self.total_sum_sq += sq;
            self.total_frames += 1;

            if self.early_frames < early_cap {
                self.early_sum_sq += sq;
                self.early_frames += 1;
            }

            self.window_sum_sq += sq;
            self.window_frames += 1;
            if self.window_frames >= window_cap {
                self.flush_window();
            }
        }
    }

    /// Close the current 1s window and update
    /// dead-air tracking.
    fn flush_window(&mut self) {
        if self.window_frames == 0 {
            return;
        }
        let rms = libm::sqrt(self.window_sum_sq / self.window_frames as f64);
        let dbfs = Self::dbfs(rms as f32);
        let window_sec = self.elapsed_secs as f32;

        if dbfs < DEAD_AIR_WINDOW_DBFS {
            // Silent window — start or extend a run.
            if self.dead_air_run_start.is_none() {
                self.dead_air_run_start = Some(window_sec);
            }
        } else if let Some(start) = self.dead_air_run_start.take() {
            // Run ended — record if long enough.
            let dur = window_sec - start;
            if dur >= DEAD_AIR_MIN_SECS {
                self.dead_air.push(DeadAirEvent {
                    start_sec: start,
                    duration_sec: dur,
                });
            }
        }

        self.elapsed_secs += 1.0;
        self.window_sum_sq = 0.0;
        self.window_frames = 0;
    }

    /// RMS (linear, 0..1) → dBFS. Matches
    /// decode_node: 20*log10(rms), floor for zero.
    fn dbfs(rms: f32) -> f32 {
        if rms > 0.0 {
            20.0 * libm::log10f(rms)
        } else {
            -f32::INFINITY
        }
    }

    /// decode_node's rms_to_lufs: 20*log10(rms) - 1
    /// (K-weighting approximation).
    fn rms_to_lufs(rms: f32) -> f32 {
        if rms <= 0.0 {
            return -100.0;
        }
        20.0 * libm::log10f(rms) - 1.0
    }

    fn early_rms(&self) -> f32 {
        if self.early_frames == 0 {
            return 0.0;
        }
        libm::sqrt(self.early_sum_sq / self.early_frames as f64) as f32
    }

    fn total_rms(&self) -> f32 {
        if self.total_frames == 0 {
            return 0.0;
        }
        libm::sqrt(self.total_sum_sq / self.total_frames as f64) as f32
    }

    /// Tier 1: call after the early window (or at
    /// EOF if the file is shorter). Digital silence
    /// → fatal, fail fast.
    pub fn tier1_verdict(&self) -> Result<(), String> {
        let dbfs = Self::dbfs(self.early_rms());
        if dbfs < DIGITAL_SILENCE_DBFS {
            return Err(format!(
                "Input validation failed: first \
                 {:.0}s are digital silence \
                 (RMS = {:.1} dBFS) — file appears \
                 dead or corrupt.",
                EARLY_WINDOW_SECS, dbfs
            ));
        }
        Ok(())
    }

    /// Tier 2: call at EOF. Reproduces
    /// decode_node's full-file silence + gain
    /// guard, byte-for-byte in wording.
    pub fn tier2_verdict(&self) -> Result<(), String> {
        let rms = self.total_rms();
        let rms_dbfs = Self::dbfs(rms);
        if rms_dbfs < SILENCE_DBFS {
            return Err(format!(
                "Input validation failed: audio is \
                 silence (RMS = {rms_dbfs:.1} dBFS)"
            ));
        }
        let rough_lufs = Self::rms_to_lufs(rms);
        let rough_gain_db = GAIN_TARGET_LUFS - rough_lufs;
        if rough_gain_db > MAX_GAIN_DB {
            return Err(format!(
                "DSP arithmetic error — \
                 normalization gain would exceed \
                 32x (input RMS = {rms_dbfs:.1} \
                 dBFS, est. gain = \
                 {rough_gain_db:.1} dB). Please \
                 ensure your audio is not \
                 practically silent."
            ));
        }
        Ok(())
    }

    /// Consume the monitor and return any dead-air
    /// events found (flushing a trailing run).
    pub fn finish(mut self) -> Vec<DeadAirEvent> {
        // Flush a trailing silent run at EOF.
        if let Some(start) = self.dead_air_run_start.take() {
            let dur = self.elapsed_secs as f32 - start;
            if dur >= DEAD_AIR_MIN_SECS {
                self.dead_air.push(DeadAirEvent {
                    start_sec: start,
                    duration_sec: dur,
                });
            }
        }
        self.dead_air
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn interleave(mono: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(mono.len() * 2);
        for &s in mono {
            out.push(s);
            out.push(s);
        }
        out
    }

    fn tone(secs: f32, amp: f32) -> Vec<f32> {
        let n = (SR as f32 * secs) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / SR as f32;
                amp * libm::sinf(2.0 * std::f32::consts::PI * 220.0 * t)
            })
            .collect()
    }

    #[test]
    fn digital_silence_fails_tier1() {
        let mut m = SignalHealthMonitor::new(SR);
        // 31s of hard zeros.
        let silence = vec![0.0f32; SR as usize * 31];
        m.observe(&interleave(&silence));
        assert!(
            m.tier1_verdict().is_err(),
            "digital silence should fail Tier 1"
        );
    }

    #[test]
    fn speech_then_silence_fails_tier2() {
        let mut m = SignalHealthMonitor::new(SR);
        // 30s of very quiet speech (0.01 amp) then long
        // silence — total RMS drags below the max-gain
        // threshold (30dB) and fails Tier 2.
        let mut sig = tone(30.0, 0.01);
        sig.extend(vec![0.0f32; SR as usize * 300]);
        m.observe(&interleave(&sig));
        assert!(m.tier1_verdict().is_ok(), "30s of tone should pass Tier 1");
        assert!(
            m.tier2_verdict().is_err(),
            "mostly-silent file should fail Tier 2"
        );
    }

    #[test]
    fn mid_gap_is_event_not_fatal() {
        let mut m = SignalHealthMonitor::new(SR);
        // tone, 5s gap, tone.
        let mut sig = tone(10.0, 0.3);
        sig.extend(vec![0.0f32; SR as usize * 5]);
        sig.extend(tone(10.0, 0.3));
        m.observe(&interleave(&sig));
        assert!(m.tier1_verdict().is_ok());
        assert!(
            m.tier2_verdict().is_ok(),
            "a gap between speech is not fatal"
        );
        let events = m.finish();
        assert!(!events.is_empty(), "the 5s gap should be a dead-air event");
        // Gap starts around 10s, lasts ~5s.
        let e = &events[0];
        assert!(
            (e.start_sec - 10.0).abs() < 2.0,
            "gap start ~10s, got {}",
            e.start_sec
        );
        assert!(
            e.duration_sec >= DEAD_AIR_MIN_SECS,
            "gap duration should exceed min"
        );
    }

    #[test]
    fn healthy_audio_passes_both_tiers() {
        let mut m = SignalHealthMonitor::new(SR);
        m.observe(&interleave(&tone(40.0, 0.3)));
        assert!(m.tier1_verdict().is_ok());
        assert!(m.tier2_verdict().is_ok());
        assert!(m.finish().is_empty(), "clean tone has no dead air");
    }
}
