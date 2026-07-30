//! ACX compliance measurement — a transcription of the Audacity "ACX Check"
//! plugin's algorithm (acx-check.ny, Will McCown 2015), which is the de facto
//! reference tool narrators compare against. ACX publishes only the LIMITS
//! (RMS -23..-18 dBFS, peak <= -3 dB, noise floor <= -60 dBFS RMS); no primary
//! source documents their analyzer's method (searched 2026-07-30). The plugin
//! is the closest inspectable spec, so we match it line for line and say so.
//!
//! Method, per the plugin source:
//!   Peak        max |sample - DC|. SAMPLE peak — the plugin does not
//!               oversample. The engine's limiter enforces TRUE peak, which
//!               is stricter; a report may carry both, labelled.
//!   RMS         whole-file, unweighted, after DC removal. Computed here in
//!               one pass via E[x^2] - mean^2 (mathematically identical to
//!               the plugin's subtract-then-RMS).
//!   NoiseFloor  8th-order Butterworth highpass at 10 Hz, then the minimum
//!               mean-square over sliding 500 ms windows at 100 ms hop, then
//!               sqrt. None if the input is shorter than 1 s (the plugin
//!               returns -1 / "selection too short").
//!
//! Known divergence from the plugin, deliberate: the plugin's min-search
//! excludes its last 7 windows as an edge-effect guard of its snd-avg
//! implementation; our sub-blocks are exact, so we search every full window.
//! On real material the quietest window is not at the file edge and the two
//! agree; a file whose only quiet moment is the final half second may differ.

use crate::analysis::pre_analysis::{butter_hp2_q, Biquad};

/// ACX published limits (dBFS). Same numbers as presets::ACX; duplicated here
/// as plain consts because sp314-dsp does not depend on lineos-types.
pub const ACX_MAX_PEAK_DB: f32 = -3.0;
pub const ACX_MAX_RMS_DB: f32 = -18.0;
pub const ACX_MIN_RMS_DB: f32 = -23.0;
pub const ACX_MAX_NOISE_FLOOR_DB: f32 = -60.0;

const HP_CUTOFF_HZ: f32 = 10.0;
const SUB_BLOCK_MS: usize = 100; // plugin hop
const WINDOW_SUB_BLOCKS: usize = 5; // 5 x 100 ms = 500 ms window
const MIN_SUB_BLOCKS: usize = 10; // plugin: len must exceed 2 x 500 ms

pub struct AcxCheckAnalyzer {
    // raw-signal accumulators (peak + RMS)
    sum: f64,
    sum_sq: f64,
    count: u64,
    max_sample: f32,
    min_sample: f32,
    // noise-floor path: HP8 -> per-sub-block mean squares, time-ordered
    hp: [Biquad; 4],
    sub_block_size: usize,
    sub_sum_sq: f64,
    sub_count: usize,
    sub_mean_sqs: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcxCheckReport {
    /// Sample peak in dBFS, DC removed — the plugin's number. The limiter's
    /// true peak is a stricter, separate measurement.
    pub sample_peak_db: f32,
    /// Whole-file unweighted RMS in dBFS, DC removed.
    pub rms_db: f32,
    /// RMS of the quietest sliding 500 ms (100 ms hop) after HP8 @ 10 Hz.
    /// None: input shorter than 1 s, matching the plugin's refusal.
    pub noise_floor_db: Option<f32>,
}

impl AcxCheckReport {
    pub fn passes_acx(&self) -> bool {
        self.sample_peak_db <= ACX_MAX_PEAK_DB
            && self.rms_db <= ACX_MAX_RMS_DB
            && self.rms_db >= ACX_MIN_RMS_DB
            && matches!(self.noise_floor_db, Some(nf) if nf <= ACX_MAX_NOISE_FLOOR_DB)
    }
}

impl AcxCheckAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        // 8th-order Butterworth = 4 cascaded 2nd-order sections whose Qs come
        // from the pole angles theta_k = (2k+1) * pi / 16, Q = 1 / (2 cos theta).
        // Computed, not transcribed: 0.5098, 0.6013, 0.9000, 2.5629.
        let hp = core::array::from_fn(|k| {
            let theta = (2 * k + 1) as f32 * core::f32::consts::PI / 16.0;
            let q = 1.0 / (2.0 * libm::cosf(theta));
            butter_hp2_q(HP_CUTOFF_HZ, sample_rate as f32, q)
        });
        Self {
            sum: 0.0,
            sum_sq: 0.0,
            count: 0,
            max_sample: f32::MIN,
            min_sample: f32::MAX,
            hp,
            sub_block_size: (sample_rate as usize * SUB_BLOCK_MS) / 1000,
            sub_sum_sq: 0.0,
            sub_count: 0,
            sub_mean_sqs: Vec::new(),
        }
    }

    pub fn feed_chunk(&mut self, chunk: &[f32]) {
        for &s in chunk {
            self.sum += s as f64;
            self.sum_sq += (s as f64) * (s as f64);
            self.count += 1;
            if s > self.max_sample {
                self.max_sample = s;
            }
            if s < self.min_sample {
                self.min_sample = s;
            }
            // noise-floor path
            let mut y = s;
            for section in self.hp.iter_mut() {
                y = section.process(y);
            }
            self.sub_sum_sq += (y as f64) * (y as f64);
            self.sub_count += 1;
            if self.sub_count == self.sub_block_size {
                self.sub_mean_sqs
                    .push((self.sub_sum_sq / self.sub_block_size as f64) as f32);
                self.sub_sum_sq = 0.0;
                self.sub_count = 0;
            }
        }
    }

    pub fn finish(self) -> AcxCheckReport {
        if self.count == 0 {
            return AcxCheckReport {
                sample_peak_db: -144.0,
                rms_db: -144.0,
                noise_floor_db: None,
            };
        }
        let mean = self.sum / self.count as f64;
        // peak after DC removal, exactly: max|x - mean| = max(max-mean, mean-min)
        let peak = libm::fmax(self.max_sample as f64 - mean, mean - self.min_sample as f64) as f32;
        // variance = E[x^2] - mean^2 == mean square of the DC-removed signal
        let mean_sq = (self.sum_sq / self.count as f64 - mean * mean).max(0.0);
        let rms = libm::sqrt(mean_sq) as f32;

        let noise_floor_db = if self.sub_mean_sqs.len() < MIN_SUB_BLOCKS {
            None
        } else {
            // sliding 500 ms window = mean of 5 consecutive sub-block mean
            // squares (equal-length blocks, so the means average exactly)
            let min_window = self
                .sub_mean_sqs
                .windows(WINDOW_SUB_BLOCKS)
                .map(|w| w.iter().sum::<f32>() / WINDOW_SUB_BLOCKS as f32)
                .fold(f32::MAX, f32::min);
            Some(to_db(libm::sqrtf(min_window)))
        };

        AcxCheckReport {
            sample_peak_db: to_db(peak),
            rms_db: to_db(rms),
            noise_floor_db,
        }
    }
}

fn to_db(linear: f32) -> f32 {
    if linear < 1e-10 {
        -144.0
    } else {
        20.0 * libm::log10f(linear)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn feed_all(sig: &[f32]) -> AcxCheckReport {
        let mut a = AcxCheckAnalyzer::new(SR);
        // deliberately odd chunk size: sub-block boundaries must not care
        for c in sig.chunks(1777) {
            a.feed_chunk(c);
        }
        a.finish()
    }

    fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
        (0..(SR as f32 * secs) as usize)
            .map(|i| amp * libm::sinf(2.0 * core::f32::consts::PI * freq * i as f32 / SR as f32))
            .collect()
    }

    /// LCG noise at a target RMS level, deterministic.
    fn noise(rms_db: f32, secs: f32, seed: &mut u32) -> Vec<f32> {
        let amp = libm::powf(10.0, rms_db / 20.0) * libm::sqrtf(3.0); // uniform: rms = amp/sqrt(3)
        (0..(SR as f32 * secs) as usize)
            .map(|_| {
                *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                ((*seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * amp
            })
            .collect()
    }

    #[test]
    fn oracle_sine_rms_and_peak() {
        let r = feed_all(&sine(997.0, 1.0, 2.0));
        assert!(
            (r.sample_peak_db - 0.0).abs() < 0.01,
            "peak {}",
            r.sample_peak_db
        );
        assert!((r.rms_db - (-3.01)).abs() < 0.02, "rms {}", r.rms_db);
    }

    #[test]
    fn oracle_dc_is_removed_from_rms() {
        // pure DC: after removal, RMS is silence
        let dc = vec![0.25_f32; SR as usize * 2];
        let r = feed_all(&dc);
        assert_eq!(r.rms_db, -144.0, "DC must not read as level");
        assert_eq!(
            r.sample_peak_db, -144.0,
            "peak is measured after DC removal"
        );
    }

    #[test]
    fn oracle_noise_floor_reads_the_pauses_not_the_speech() {
        // Synthetic narration: 2 s "speech" (997 Hz at -20 dB RMS) then a 1 s
        // "pause" of room tone at -70 dB RMS, repeated. The floor must read
        // the pause level, nowhere near the speech level.
        let mut seed = 42u32;
        let mut sig = Vec::new();
        for _ in 0..3 {
            sig.extend(sine(
                997.0,
                libm::powf(10.0, -20.0 / 20.0) * core::f32::consts::SQRT_2,
                2.0,
            ));
            sig.extend(noise(-70.0, 1.0, &mut seed));
        }
        let r = feed_all(&sig);
        let nf = r.noise_floor_db.expect("long enough");
        assert!(
            nf < -60.0,
            "floor {} should pass -60 given -70 dB pauses",
            nf
        );
        assert!(
            nf > -80.0,
            "floor {} implausibly low for -70 dB room tone",
            nf
        );
        // and RMS is dominated by the speech, far above the floor
        assert!(r.rms_db > -26.0 && r.rms_db < -18.0, "rms {}", r.rms_db);
    }

    #[test]
    fn oracle_continuous_speech_floor_is_honest_about_its_limit() {
        // No pauses at all: the "floor" is just the quietest 500 ms of tone.
        // This documents the known limitation rather than hiding it.
        let r = feed_all(&sine(997.0, 0.1, 4.0));
        let nf = r.noise_floor_db.expect("long enough");
        assert!(
            nf > -30.0,
            "with no silence the floor reads near the signal: {}",
            nf
        );
    }

    #[test]
    fn short_input_refuses_a_floor() {
        let r = feed_all(&sine(997.0, 0.5, 0.9));
        assert!(r.noise_floor_db.is_none(), "under 1 s must be None");
        // RMS and peak still report
        assert!(r.rms_db > -144.0);
    }

    #[test]
    fn passes_acx_checks_all_three() {
        let mut seed = 7u32;
        let mut sig = Vec::new();
        for _ in 0..3 {
            sig.extend(sine(
                200.0,
                libm::powf(10.0, -20.0 / 20.0) * core::f32::consts::SQRT_2,
                2.0,
            ));
            sig.extend(noise(-72.0, 1.0, &mut seed));
        }
        let r = feed_all(&sig);
        assert!(r.passes_acx(), "{r:?}");
        // and each failure mode flips it
        let loud = feed_all(&sine(997.0, 1.0, 2.0));
        assert!(!loud.passes_acx());
    }
}
