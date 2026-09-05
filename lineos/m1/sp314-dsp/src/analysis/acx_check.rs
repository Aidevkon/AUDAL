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

/// Encoder gap margins — ΑΠΟ ΜΕΤΡΗΣΗ, όχι θεωρία.
/// F-077, 2026-08-23: 60 αρχεία LibriSpeech
/// dev-clean + καθαρά ημίτονα + καθαρό ffmpeg,
/// τρεις decoders συμφωνούν στα 0.0006 dB.
/// Το χάσμα είναι ΣΤΑ BITS του LAME, ομοιόμορφο
/// ×0.9698, ανεξάρτητο περιεχομένου.
/// ΑΝΑ ΚΑΤΕΥΘΥΝΣΗ, γιατί το χάσμα είναι ΑΡΝΗΤΙΚΟ:
/// σε ceiling απομακρύνει, σε floor πλησιάζει.
///
/// Ισχύει ΜΟΝΟ όπου μεσολαβεί πραγματικός encoder ανάμεσα
/// στη μέτρηση και στο παραδοτέο — δηλ. στο
/// `levels_within_limits_with_margin()` παρακάτω, ΟΧΙ στο `levels_within_limits()`.
/// Το `levels_within_limits()` το καλεί και το `input_acx_compliant`
/// (certificate_node.rs), που κρίνει το INPUT πριν από
/// οποιοδήποτε render/encode· εκεί δεν υπάρχει χάσμα να
/// αντισταθμιστεί, ΒΗΜΑ 0 F-implement-margin 2026-08-24.
/// MEASURED: n=59 2026-08-24 [ref: F-077 · tests/external_acx_ffmpeg_agreement.rs]
pub const ACX_MARGIN_RMS_FLOOR_DB: f32 = 0.35; // n=59, worst −0.280
pub const ACX_MARGIN_RMS_CEILING_DB: f32 = 0.10; // ⚠ n=1, worst +0.027
pub const ACX_MARGIN_PEAK_DB: f32 = 0.20; // ⚠ n=1, worst +0.037·
                                           // ⚠ ΚΑΙ: το F-079
                                           // μέτρησε 4× συχνοτική
                                           // εξάρτηση, το δείγμα
                                           // ήταν 16kHz· ΚΑΙ το
                                           // ερώτημα sample-vs-
                                           // true peak ΑΝΟΙΧΤΟ
                                           // (ΑΤΖΕΝΤΑ 23/08)
pub const ACX_MARGIN_NOISE_FLOOR_DB: f32 = 0.00; // ασφαλής φορά·
                                                  // μετρημένο σε −82,
                                                  // ζώνη −60 αμέτρητη

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
    /// The start position of the quietest 500 ms window used for the noise floor,
    /// in samples at the analyzer's rate. Note: if the analyzer runs at 44.1kHz
    /// in the export path vs 48kHz in the trunk, this sample count is relative
    /// to THAT specific stream's sample rate.
    /// None: input shorter than 1 s.
    pub quietest_window_start_frame: Option<usize>,
}

/// One margin-adjusted per-metric judgment: the published requirement, our
/// measured F-077 margin, and the resulting verdict — the single source
/// `levels_within_limits_with_margin()` and any caller building a `§5.3` record (e.g.
/// `deliver.rs`'s `DeliveryCheck`) both read from, so the rule exists in
/// exactly one place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcxMarginCheck {
    /// "rms" | "peak" | "noise_floor".
    pub metric: &'static str,
    pub measured_db: f32,
    /// The published requirement, no margin applied.
    pub required_db: f32,
    /// "min" | "max" — which direction `required_db` bounds.
    pub bound: &'static str,
    /// The F-077 margin actually applied for this bound.
    pub margin_applied_db: f32,
    pub verdict: bool,
}

impl AcxCheckReport {
    /// ΜΕΡΙΚΟ ΕΞ ΟΡΙΣΜΟΥ: κρίνει ΜΟΝΟ τα τρία επίπεδα σήματος
    /// (rms · peak · noise floor). ΔΕΝ βλέπει spacing/format — ζουν σε
    /// άλλο crate. Η ΠΛΗΡΗΣ ετυμηγορία είναι το
    /// `DeliveryVerdict::compose` (m0d, 1a40772).
    /// ΜΗΝ το χρησιμοποιήσεις ως τελική κρίση.
    ///
    /// ⚠ ΜΕΤΟΝΟΜΑΣΙΑ ΑΠΟ `passes_acx`, 2026-08-25. Το παλιό όνομα έλεγε
    /// «περνάει το ΑΡΧΕΙΟ το ACX» ενώ απαντά «είναι τα τρία επίπεδα
    /// εντός ορίων;» — F-074 σε boolean αντί για hash. ΚΑΙ: το «acx»
    /// μέσα σε συνάρτηση του sp314-dsp είναι εμπορικό όνομα σε στρώμα
    /// DSP (§5.1α, κεφάλι/πλοκάμι). Το νέο όνομα δηλώνει ΕΜΒΕΛΕΙΑ.
    pub fn levels_within_limits(&self) -> bool {
        self.sample_peak_db <= ACX_MAX_PEAK_DB
            && self.rms_db <= ACX_MAX_RMS_DB
            && self.rms_db >= ACX_MIN_RMS_DB
            && matches!(self.noise_floor_db, Some(nf) if nf <= ACX_MAX_NOISE_FLOOR_DB)
    }

    /// The per-metric breakdown behind `levels_within_limits_with_margin()`. Omits the
    /// noise-floor entry entirely when `noise_floor_db` is `None` — absence
    /// rule (§5.2): a metric that was not measured produces no record, not a
    /// fabricated one.
    pub fn margin_checks(&self) -> Vec<AcxMarginCheck> {
        let mut checks = vec![
            AcxMarginCheck {
                metric: "rms",
                measured_db: self.rms_db,
                required_db: ACX_MIN_RMS_DB,
                bound: "min",
                margin_applied_db: ACX_MARGIN_RMS_FLOOR_DB,
                verdict: self.rms_db >= ACX_MIN_RMS_DB + ACX_MARGIN_RMS_FLOOR_DB,
            },
            AcxMarginCheck {
                metric: "rms",
                measured_db: self.rms_db,
                required_db: ACX_MAX_RMS_DB,
                bound: "max",
                margin_applied_db: ACX_MARGIN_RMS_CEILING_DB,
                verdict: self.rms_db <= ACX_MAX_RMS_DB - ACX_MARGIN_RMS_CEILING_DB,
            },
            AcxMarginCheck {
                metric: "peak",
                measured_db: self.sample_peak_db,
                required_db: ACX_MAX_PEAK_DB,
                bound: "max",
                margin_applied_db: ACX_MARGIN_PEAK_DB,
                verdict: self.sample_peak_db <= ACX_MAX_PEAK_DB - ACX_MARGIN_PEAK_DB,
            },
        ];
        if let Some(nf) = self.noise_floor_db {
            checks.push(AcxMarginCheck {
                metric: "noise_floor",
                measured_db: nf,
                required_db: ACX_MAX_NOISE_FLOOR_DB,
                bound: "max",
                margin_applied_db: ACX_MARGIN_NOISE_FLOOR_DB,
                verdict: nf <= ACX_MAX_NOISE_FLOOR_DB - ACX_MARGIN_NOISE_FLOOR_DB,
            });
        }
        checks
    }

    /// Same verdict as `levels_within_limits()`, but against thresholds that have
    /// already absorbed the measured LAME encoder gap (F-077) — i.e. the
    /// gap between this pre-encode report and what the decoded mp3 will
    /// actually measure. Use ONLY where `self` is the exact buffer about to
    /// be lossy-encoded (e.g. `deliver.rs`'s manifest verdict, via
    /// `export_mp3_acx`'s pre-LAME report). Do NOT use this for
    /// `input_acx_compliant` — that report is measured on raw input, before
    /// any render or encode step, so there is no encoder gap to correct
    /// there; see the doc comment on the `ACX_MARGIN_*` consts above.
    ///
    /// Missing noise floor still fails (matching `levels_within_limits()`): the
    /// absence rule governs the §5.3 *record*, not this boolean.
    /// ΜΕΡΙΚΟ ΕΞ ΟΡΙΣΜΟΥ — ίδια εμβέλεια με το `levels_within_limits`,
    /// με τα κατώφλια προσαρμοσμένα στο μετρημένο χάσμα του encoder
    /// (F-077). ΔΕΝ βλέπει spacing/format. Η ΠΛΗΡΗΣ ετυμηγορία είναι το
    /// `DeliveryVerdict::compose` (m0d, 1a40772).
    /// ΜΗΝ το χρησιμοποιήσεις ως τελική κρίση.
    ///
    /// ⚠ ΜΕΤΟΝΟΜΑΣΙΑ ΑΠΟ `passes_acx_with_margin`, 2026-08-25.
    pub fn levels_within_limits_with_margin(&self) -> bool {
        self.noise_floor_db.is_some() && self.margin_checks().iter().all(|c| c.verdict)
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
                quietest_window_start_frame: None,
            };
        }
        let mean = self.sum / self.count as f64;
        // peak after DC removal, exactly: max|x - mean| = max(max-mean, mean-min)
        let peak = libm::fmax(self.max_sample as f64 - mean, mean - self.min_sample as f64) as f32;
        // variance = E[x^2] - mean^2 == mean square of the DC-removed signal
        let mean_sq = (self.sum_sq / self.count as f64 - mean * mean).max(0.0);
        let rms = libm::sqrt(mean_sq) as f32;

        let (noise_floor_db, quietest_window_start_frame) =
            if self.sub_mean_sqs.len() < MIN_SUB_BLOCKS {
                (None, None)
            } else {
                // sliding 500 ms window = mean of 5 consecutive sub-block mean
                // squares (equal-length blocks, so the means average exactly)
                let mut min_window = f32::MAX;
                let mut min_idx = 0;
                for (i, w) in self.sub_mean_sqs.windows(WINDOW_SUB_BLOCKS).enumerate() {
                    let sum = w.iter().sum::<f32>() / WINDOW_SUB_BLOCKS as f32;
                    if sum < min_window {
                        min_window = sum;
                        min_idx = i;
                    }
                }
                (
                    Some(to_db(libm::sqrtf(min_window))),
                    Some(min_idx * self.sub_block_size),
                )
            };

        AcxCheckReport {
            sample_peak_db: to_db(peak),
            rms_db: to_db(rms),
            noise_floor_db,
            quietest_window_start_frame,
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
    fn levels_within_limits_checks_all_three() {
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
        assert!(r.levels_within_limits(), "{r:?}");
        // and each failure mode flips it
        let loud = feed_all(&sine(997.0, 1.0, 2.0));
        assert!(!loud.levels_within_limits());
    }

    /// ΠΡΟΒΛΕΨΗ (γραμμένη πριν το τρέξιμο):
    /// rms −22.9 dB είναι ΜΕΣΑ στο ονομαστικό παράθυρο [−23, −18] αλλά
    /// ΕΞΩ από το προσαρμοσμένο [−23+0.35, −18−0.10] = [−22.65, −18.10]
    /// (−22.9 < −22.65). Άρα, στο ΙΔΙΟ report: levels_within_limits() == true ΚΑΙ
    /// levels_within_limits_with_margin() == false. Peak/floor τίθενται άνετα μέσα
    /// σε αμφότερα τα παράθυρα ώστε το RMS να είναι ο μόνος κριτής.
    #[test]
    fn margin_flips_verdict_at_the_edge_without_touching_nominal_levels() {
        let edge = AcxCheckReport {
            sample_peak_db: -10.0,
            rms_db: -22.9,
            noise_floor_db: Some(-70.0),
            quietest_window_start_frame: Some(0),
        };
        assert!(edge.levels_within_limits(), "{edge:?}");
        assert!(!edge.levels_within_limits_with_margin(), "{edge:?}");
    }

    /// ΠΡΟΒΛΕΨΗ: rms −20.5 (μέσο του ονομαστικού παραθύρου) είναι ΜΕΣΑ
    /// σε αμφότερα τα παράθυρα → PASS και στα δύο. Αποδεικνύει ότι το
    /// margin δεν σπάει κανονικό υλικό.
    #[test]
    fn margin_agrees_with_nominal_at_mid_window() {
        let mid = AcxCheckReport {
            sample_peak_db: -10.0,
            rms_db: -20.5,
            noise_floor_db: Some(-70.0),
            quietest_window_start_frame: Some(0),
        };
        assert!(mid.levels_within_limits(), "{mid:?}");
        assert!(mid.levels_within_limits_with_margin(), "{mid:?}");
    }

    #[test]
    fn oracle_quietest_window_position() {
        let mut seed = 42u32;
        let mut sig = Vec::new();
        // Loud 0-2s
        sig.extend(sine(
            997.0,
            libm::powf(10.0, -10.0 / 20.0) * core::f32::consts::SQRT_2,
            2.0,
        ));
        // -70 dB noise 2-2.5s
        sig.extend(noise(-70.0, 0.5, &mut seed));
        // Loud 2.5-4s
        sig.extend(sine(
            997.0,
            libm::powf(10.0, -10.0 / 20.0) * core::f32::consts::SQRT_2,
            1.5,
        ));

        let r = feed_all(&sig);
        let start_frame = r.quietest_window_start_frame.expect("long enough");
        let start_sec = start_frame as f32 / SR as f32;

        // Position lands inside [2.0s, 2.5s] ±1 window hop (100ms = 0.1s)
        // Since the quiet part is exactly 500ms (the window length) at 2.0s,
        // the start should be very close to 2.0s.
        assert!(
            start_sec >= 1.9 && start_sec <= 2.1,
            "quiet window started at {}s, expected around 2.0s",
            start_sec
        );

        // All-loud signal => position is Some (the least-loud window) consistent with floor
        let loud_sig = sine(
            997.0,
            libm::powf(10.0, -10.0 / 20.0) * core::f32::consts::SQRT_2,
            4.0,
        );
        let r_loud = feed_all(&loud_sig);
        assert!(
            r_loud.quietest_window_start_frame.is_some(),
            "all loud signal must return Some position"
        );
    }
}
