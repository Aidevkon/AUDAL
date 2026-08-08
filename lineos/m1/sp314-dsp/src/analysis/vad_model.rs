use crate::analysis::vad_features::VadFeatures;

// --- Module-Level Constants ---

pub const K_SNR: f32 = 0.15;
pub const SNR_CENTER: f32 = 6.0;

pub const MS_SCALE_SPEECH: f32 = 0.15;
pub const MS_MU_NOISE: f32 = 0.6;
pub const MS_SIG_NOISE: f32 = 0.4;
pub const MS_NORM: f32 = 1.899_767_8;

pub const DRMS_MU_SPEECH: f32 = 1.0;
pub const DRMS_SIG_SPEECH: f32 = 1.0;
pub const DRMS_MU_NOISE: f32 = 0.2;
pub const DRMS_SIG_NOISE: f32 = 0.3;
pub const DRMS_LOG_SIG_RATIO: f32 = -1.2039728;

pub const TEMPERATURE: f32 = 2.0;

pub const ENTER: f32 = 0.70;
pub const EXIT: f32 = 0.30;
pub const HOLD_FRAMES: usize = 15; // 150 ms

pub const ATTACK: f32 = 0.3; // ~33 ms to catch onset
pub const RELEASE: f32 = 0.02; // ~500 ms so music doesn't snap back

// --- Structs ---

pub struct VadContext {
    pub noise_floor_dbfs: f32,
    /// |rms_t - rms_{t-3}|, computed by the classifier from its own ring
    pub rms_delta_30ms: f32,
    pub transient_rate: Option<f32>,
}

pub trait LikelihoodModel {
    /// Signed log-odds in favour of speech for ONE frame.
    /// MUST be pure: same inputs -> same output, no interior state.
    /// Raw, UNCALIBRATED signed log-odds. Do NOT apply
    /// temperature here — it must stay composable (Stage c3
    /// composes FixedPriors with an MFCC term).
    fn log_odds(&self, f: &VadFeatures, ctx: &VadContext) -> f32;

    /// Deflation for Naive-Bayes overcounting. 2.0 suits four
    /// correlated scalar sensors; a model with more dimensions
    /// declares its own.
    fn temperature(&self) -> f32 {
        TEMPERATURE
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FixedPriors;

impl FixedPriors {
    pub fn log_odds_terms(&self, f: &VadFeatures, ctx: &VadContext) -> [f32; 5] {
        // ROBUSTNESS GUARD
        if f.rms_db.is_nan()
            || f.rms_db.is_infinite()
            || f.spectral_flatness.is_nan()
            || f.spectral_flatness.is_infinite()
            || f.mid_side_ratio.is_nan()
            || f.mid_side_ratio.is_infinite()
            || ctx.noise_floor_dbfs.is_nan()
            || ctx.noise_floor_dbfs.is_infinite()
            || ctx.rms_delta_30ms.is_nan()
            || ctx.rms_delta_30ms.is_infinite()
        {
            return [0.0; 5];
        }

        // --- (a) SNR ---
        // S3: clamp noise_floor to >= -70.0 dBFS (digital silence is not a room).
        // Measured floor was -94.85 which produced meaningless 64 dB SNR values.
        let effective_floor = ctx.noise_floor_dbfs.max(-70.0);
        let snr = f.rms_db - effective_floor;
        // SNR separates signal from SILENCE, not speech from music.
        // Measured: music sat 17.35 dB above its floor, speech 6.59. Its remaining job is keeping room tone out.
        let l_snr = (K_SNR * (snr - SNR_CENTER)).clamp(-1.0, 1.0);

        // --- (b) Spectral Flatness (Bimodal Non-Speech) ---
        // S1: Retuned from measured distributions (VADVAL FLATRAW).
        // Speech narration actual p10-p90 = 0.005-0.115, medians 0.014/0.024.
        const FLAT_MU_SPEECH: f32 = 0.03;
        const FLAT_SIG_SPEECH: f32 = 0.04;
        const LN_SIG_SPEECH: f32 = -3.218_876; // ln(0.04) — same f32 bit pattern as the surgery's value, clippy-shortened

        // Noise/room tone actual p50=0.22-0.35. We use one class for all non-speech.
        const FLAT_MU_HISS: f32 = 0.28;
        const FLAT_SIG_HISS: f32 = 0.08;
        const LN_SIG_HISS: f32 = -2.5257286; // ln(0.08)

        let x = f.spectral_flatness;
        let z_s = (x - FLAT_MU_SPEECH) / FLAT_SIG_SPEECH;
        let z_n = (x - FLAT_MU_HISS) / FLAT_SIG_HISS;
        let log_sig_n = LN_SIG_HISS;

        // now the primary separator
        let l_flat =
            (0.5 * z_n * z_n - 0.5 * z_s * z_s + log_sig_n - LN_SIG_SPEECH).clamp(-8.0, 8.0);

        // --- (c) Side/Mid Ratio ---
        // S2 mono abstention: mono input carries no spatial evidence
        let l_ms = if f.mid_side_ratio < 1e-6 {
            0.0
        } else {
            let r = f.mid_side_ratio.clamp(0.0, 1.0);
            let z_ms_noise = (r - MS_MU_NOISE) / MS_SIG_NOISE;
            (-(r / MS_SCALE_SPEECH) + 0.5 * (z_ms_noise * z_ms_noise) + MS_NORM).clamp(-0.5, 0.5)
        };

        // --- (d) |ΔRMS| over 30ms ---
        let z_s_drms = (ctx.rms_delta_30ms - DRMS_MU_SPEECH) / DRMS_SIG_SPEECH;
        let z_n_drms = (ctx.rms_delta_30ms - DRMS_MU_NOISE) / DRMS_SIG_NOISE;
        // medians 1.169 vs 1.147 — no separation on this material
        let l_drms = (0.5 * (z_n_drms * z_n_drms) - 0.5 * (z_s_drms * z_s_drms)
            + DRMS_LOG_SIG_RATIO)
            .clamp(-0.5, 0.5);

        // The clamps on the individual terms ARE the weighting policy, tunable by ear.
        // Which sensor we trust came from measurement, not intuition.
        // l_ms and l_drms are clamped tight so their constant bias cannot outvote flatness.

        // --- (e) Transient Rate (Rolling 1s) ---
        // ABSTAIN (0.0) when snr_db < 6.0 — measured: dream quiet shows
        // rate p50=3/p90=10 because the ratio-based detector fires on
        // near-silence flutter (RATERAW|dream|NONSPEECH).
        let l_rate = if snr < 6.0 {
            0.0
        } else if let Some(rate) = ctx.transient_rate {
            // speech: N(mu=5.5, sig=3.5)
            // music:  N(mu=1.0, sig=1.5)
            const RATE_MU_SPEECH: f32 = 5.5;
            const RATE_SIG_SPEECH: f32 = 3.5;
            const RATE_MU_MUSIC: f32 = 1.0;
            const RATE_SIG_MUSIC: f32 = 1.5;

            let z_s = (rate - RATE_MU_SPEECH) / RATE_SIG_SPEECH;
            let z_m = (rate - RATE_MU_MUSIC) / RATE_SIG_MUSIC;
            const RATE_LN_SIG_RATIO: f32 = -0.84729786; // ln(1.5) - ln(3.5)

            (0.5 * (z_m * z_m) - 0.5 * (z_s * z_s) + RATE_LN_SIG_RATIO).clamp(-1.5, 1.5)
        } else {
            // Abstain during the first second warmup (until ring is full)
            0.0
        };

        [l_snr, l_flat, l_ms, l_drms, l_rate]
    }
}

impl LikelihoodModel for FixedPriors {
    fn log_odds(&self, f: &VadFeatures, ctx: &VadContext) -> f32 {
        let terms = self.log_odds_terms(f, ctx);
        terms[0] + terms[1] + terms[2] + terms[3] + terms[4]
    }
}

pub struct VadObservation {
    pub frame_index: u64,
    pub posterior: f32,
    pub is_speech: bool,
    pub duck_gain: f32,
    pub rms_db: f32,
    pub spectral_flatness: f32,
    pub mid_side_ratio: f32,
    pub rms_delta_30ms: f32,
    pub noise_floor_dbfs: f32,
    pub phi1_p: Option<f32>,
}

pub struct VadDecision {
    pub posterior: f32,
    pub is_speech: bool,
    pub duck_gain: f32,
    pub rms_delta_30ms: f32,
    pub transient_rate: Option<f32>,
}

pub struct VadClassifier<M: LikelihoodModel> {
    model: M,
    rms_ring: [f32; 3],
    ring_filled: usize,
    ring_idx: usize,
    transient_ring: [i32; 100],
    trans_ring_idx: usize,
    trans_ring_filled: usize,
    pub is_speech: bool,
    hold_counter: usize,
    pub duck_gain: f32,
}

impl<M: LikelihoodModel> VadClassifier<M> {
    pub fn new(model: M) -> Self {
        Self {
            model,
            rms_ring: [0.0; 3],
            ring_filled: 0,
            ring_idx: 0,
            transient_ring: [0; 100],
            trans_ring_idx: 0,
            trans_ring_filled: 0,
            is_speech: false,
            hold_counter: 0,
            duck_gain: 0.0,
        }
    }

    pub fn process(&mut self, f: &VadFeatures, noise_floor_dbfs: f32) -> VadDecision {
        let valid_rms = if f.rms_db.is_finite() {
            f.rms_db
        } else if self.ring_filled > 0 {
            self.rms_ring[(self.ring_idx + 2) % 3]
        } else {
            -144.0
        };

        let rms_delta_30ms = if self.ring_filled >= 3 {
            libm::fabsf(valid_rms - self.rms_ring[self.ring_idx])
        } else {
            0.0
        };

        self.rms_ring[self.ring_idx] = valid_rms;
        self.ring_idx = (self.ring_idx + 1) % 3;
        if self.ring_filled < 3 {
            self.ring_filled += 1;
        }

        let trans_count = (f.transient_density * 0.01).round() as i32;
        self.transient_ring[self.trans_ring_idx] = trans_count;
        self.trans_ring_idx = (self.trans_ring_idx + 1) % 100;
        if self.trans_ring_filled < 100 {
            self.trans_ring_filled += 1;
        }

        let transient_rate = if self.trans_ring_filled == 100 {
            Some(self.transient_ring.iter().sum::<i32>() as f32)
        } else {
            None
        };

        let ctx = VadContext {
            noise_floor_dbfs,
            rms_delta_30ms,
            transient_rate,
        };

        let log_odds = self.model.log_odds(f, &ctx);
        let posterior = 1.0 / (1.0 + libm::expf(-log_odds / self.model.temperature()));

        if !self.is_speech && posterior > ENTER {
            self.is_speech = true;
            self.hold_counter = HOLD_FRAMES;
        } else if self.is_speech {
            if posterior > EXIT {
                self.hold_counter = HOLD_FRAMES;
            } else if self.hold_counter > 0 {
                self.hold_counter -= 1;
            } else {
                self.is_speech = false;
            }
        }

        let target = if self.is_speech { 1.0 } else { 0.0 };
        let a = if target > self.duck_gain {
            ATTACK
        } else {
            RELEASE
        };
        self.duck_gain += (target - self.duck_gain) * a;

        VadDecision {
            posterior,
            is_speech: self.is_speech,
            duck_gain: self.duck_gain,
            rms_delta_30ms,
            transient_rate,
        }
    }
}

// ── Oracles (Tests) ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_features() -> VadFeatures {
        VadFeatures {
            spectral_flatness: 0.5,
            rms_db: -40.0,
            rms_delta_db: 0.0,
            mid_side_ratio: 0.5,
            transient_density: 0.0,
            mfcc: [0.0; 13],
        }
    }

    #[test]
    fn oracle_snr_monotonic() {
        let ctx = VadContext {
            noise_floor_dbfs: -60.0,
            rms_delta_30ms: 1.0,
            transient_rate: None,
        };

        let mut f_floor = dummy_features();
        f_floor.rms_db = -60.0;
        let l1 = FixedPriors.log_odds(&f_floor, &ctx);

        let mut f_plus10 = dummy_features();
        f_plus10.rms_db = -50.0;
        let l2 = FixedPriors.log_odds(&f_plus10, &ctx);

        let mut f_plus20 = dummy_features();
        f_plus20.rms_db = -40.0;
        let l3 = FixedPriors.log_odds(&f_plus20, &ctx);

        assert!(l2 > l1, "SNR +10 should increase log-odds");
        assert!(l3 > l2, "SNR +20 should further increase log-odds");
    }

    #[test]
    fn oracle_music_vs_speech() {
        let mut ctx = VadContext {
            noise_floor_dbfs: 0.0,
            rms_delta_30ms: DRMS_MU_NOISE,
            transient_rate: None,
        };

        let mut f_speech = dummy_features();
        // VADVAL FLATRAW measured narration speech median: ~0.014-0.024
        f_speech.spectral_flatness = 0.02;
        f_speech.rms_db = 6.59;
        f_speech.mid_side_ratio = 0.297;
        let l_speech = FixedPriors.log_odds(&f_speech, &ctx);

        ctx.rms_delta_30ms = 1.169;
        let mut f_music = dummy_features();
        // pure-tonal music is the flatness term's documented blind spot (F-061 / bimodal test);
        // this oracle guards the TYPICAL music case, measured from bodleasons_mid.
        // VADVAL measured MUSIC: FLATRAW p50 ~0.16, SNR p50 ~10.9
        f_music.spectral_flatness = 0.16;
        f_music.rms_db = 10.9;
        f_music.mid_side_ratio = 0.338;
        let l_music = FixedPriors.log_odds(&f_music, &ctx);

        assert!(l_speech > 0.0, "Speech log-odds {} should be > 0", l_speech);
        assert!(l_music < 0.0, "Music log-odds {} should be < 0", l_music);
    }

    #[test]
    fn oracle_rate_positive() {
        let mut f = dummy_features();
        f.rms_db = -30.0;
        let ctx = VadContext {
            noise_floor_dbfs: -60.0,
            rms_delta_30ms: DRMS_MU_SPEECH,
            transient_rate: Some(6.0),
        };
        let terms = FixedPriors.log_odds_terms(&f, &ctx);
        assert!(
            terms[4] > 0.0,
            "rate 6 at snr 30 should be positive, got {}",
            terms[4]
        );
    }

    #[test]
    fn oracle_rate_negative() {
        let mut f = dummy_features();
        f.rms_db = -30.0;
        let ctx = VadContext {
            noise_floor_dbfs: -60.0,
            rms_delta_30ms: DRMS_MU_SPEECH,
            transient_rate: Some(1.0),
        };
        let terms = FixedPriors.log_odds_terms(&f, &ctx);
        assert!(
            terms[4] < 0.0,
            "rate 1 at snr 30 should be negative, got {}",
            terms[4]
        );
    }

    #[test]
    fn oracle_rate_abstain_snr() {
        let mut f = dummy_features();
        f.rms_db = -58.0;
        let ctx = VadContext {
            noise_floor_dbfs: -60.0, // snr = 2.0 < 6.0
            rms_delta_30ms: DRMS_MU_SPEECH,
            transient_rate: Some(10.0),
        };
        let terms = FixedPriors.log_odds_terms(&f, &ctx);
        assert_eq!(
            terms[4], 0.0,
            "rate 10 at snr 2 should exactly abstain (0.0)"
        );
    }

    #[test]
    fn oracle_rate_abstain_warmup() {
        let mut f = dummy_features();
        f.rms_db = -30.0;
        let ctx = VadContext {
            noise_floor_dbfs: -60.0, // snr = 30.0 >= 6.0
            rms_delta_30ms: DRMS_MU_SPEECH,
            transient_rate: None,
        };
        let terms = FixedPriors.log_odds_terms(&f, &ctx);
        assert_eq!(terms[4], 0.0, "warmup (None) should exactly abstain (0.0)");
    }

    #[test]
    fn oracle_flatness_bimodal() {
        let ctx = VadContext {
            noise_floor_dbfs: 0.0,
            rms_delta_30ms: 0.2,
            transient_rate: None,
        };
        let mut f = dummy_features();
        f.rms_db = 6.0; // SNR = 6.0 => l_snr = 0.0

        // Measured room tone/hiss p50 is ~0.28
        f.spectral_flatness = 0.28;
        let l_hiss = FixedPriors.log_odds(&f, &ctx);

        // VADVAL FLATRAW measured narration speech median: ~0.014-0.024
        f.spectral_flatness = 0.02;
        let l_speech = FixedPriors.log_odds(&f, &ctx);

        // Documented limitation: tonal content at 0.03 reads speech-like BY DESIGN of this term alone.
        // Measured reality says close-mic narration and pure tones are indistinguishable on flatness;
        // disambiguation is the transient-density sensor's future job (F-061).
        f.spectral_flatness = 0.03;
        let l_tonal = FixedPriors.log_odds(&f, &ctx);

        assert!(l_speech > 0.0, "Speech should be > 0 (was {})", l_speech);
        assert!(
            l_hiss < 0.0,
            "Broadband hiss should be < 0 (was {})",
            l_hiss
        );
        assert!(
            l_tonal > 0.0,
            "Tonal music falsely reads as speech on flatness alone (was {})",
            l_tonal
        );
    }

    #[test]
    fn oracle_flatness_sign() {
        let ctx = VadContext {
            noise_floor_dbfs: -60.0,
            rms_delta_30ms: 1.0,
            transient_rate: None,
        };

        let mut f_speech = dummy_features();
        f_speech.spectral_flatness = 0.20;
        let l_speech = FixedPriors.log_odds(&f_speech, &ctx);

        let mut f_noise = dummy_features();
        f_noise.spectral_flatness = 0.80;
        let l_noise = FixedPriors.log_odds(&f_noise, &ctx);

        assert!(
            l_speech > l_noise,
            "Low flatness strictly implies higher log-odds"
        );
    }

    #[test]
    fn oracle_mid_side_sign() {
        let ctx = VadContext {
            noise_floor_dbfs: -60.0,
            rms_delta_30ms: 1.0,
            transient_rate: None,
        };

        let mut f_center = dummy_features();
        f_center.mid_side_ratio = 0.02;
        let l_center = FixedPriors.log_odds(&f_center, &ctx);

        let mut f_wide = dummy_features();
        f_wide.mid_side_ratio = 1.0;
        let l_wide = FixedPriors.log_odds(&f_wide, &ctx);

        assert!(l_center > l_wide, "Centered audio strictly higher log-odds");
    }

    #[test]
    fn oracle_ordering() {
        let ctx = VadContext {
            noise_floor_dbfs: -60.0,
            rms_delta_30ms: 0.5,
            transient_rate: None,
        };

        let f_speech = VadFeatures {
            // VADVAL FLATRAW measured narration speech median: ~0.014-0.024
            spectral_flatness: 0.02,
            rms_db: -20.0,
            rms_delta_db: 1.0,
            mid_side_ratio: 0.0,
            transient_density: 0.0,
            mfcc: [0.0; 13],
        };
        let l_speech = FixedPriors.log_odds(&f_speech, &ctx);
        let p_speech = 1.0 / (1.0 + libm::expf(-l_speech / FixedPriors.temperature()));

        let f_hiss = VadFeatures {
            spectral_flatness: 0.28,
            rms_db: -58.0,
            rms_delta_db: 0.1,
            mid_side_ratio: 0.5,
            transient_density: 0.0,
            mfcc: [0.0; 13],
        };
        let l_hiss = FixedPriors.log_odds(&f_hiss, &ctx);
        let p_hiss = 1.0 / (1.0 + libm::expf(-l_hiss / FixedPriors.temperature()));

        // This is the ONLY test asserting absolute values to bound the thresholds,
        // and it ensures the priors land on the correct side of the real decision thresholds.
        assert!(p_speech > ENTER, "Clean speech decisively exceeds ENTER");
        assert!(p_hiss < EXIT, "Static hiss decisively below EXIT");
    }

    struct ScriptedModel {
        fixed_log_odds: f32,
    }

    impl LikelihoodModel for ScriptedModel {
        fn log_odds(&self, _f: &VadFeatures, _ctx: &VadContext) -> f32 {
            self.fixed_log_odds
        }
    }

    #[test]
    fn oracle_hysteresis_no_flutter() {
        let mut classifier = VadClassifier::new(ScriptedModel {
            fixed_log_odds: 0.0,
        });
        let f = dummy_features();

        // Hover around posterior = 0.5 (log_odds = 0.0)
        assert!(!classifier.is_speech);
        for _ in 0..20 {
            let d = classifier.process(&f, -60.0);
            assert!(!d.is_speech, "Must not flutter on ambiguous input");
        }

        // Force a definitive ENTER (log_odds = 10.0 -> p > 0.99)
        classifier.model.fixed_log_odds = 10.0; // Pre-temperature
        let d_enter = classifier.process(&f, -60.0);
        assert!(d_enter.is_speech, "Latches on strong evidence");

        // Force a weak frame (log_odds = 0.0 -> p = 0.5) - should HOLD
        classifier.model.fixed_log_odds = 0.0;
        let d_hold = classifier.process(&f, -60.0);
        assert!(d_hold.is_speech, "Holds through ambiguity");

        // Force definitive EXIT (log_odds = -10.0 -> p < 0.01)
        classifier.model.fixed_log_odds = -10.0; // Pre-temperature
        for i in 0..20 {
            let d = classifier.process(&f, -60.0);
            if i < 15 {
                assert!(d.is_speech, "Holds for exactly HOLD_FRAMES (15)");
            } else {
                assert!(!d.is_speech, "Releases after HOLD_FRAMES expires");
            }
        }
    }

    #[test]
    fn oracle_duck_gain_shape() {
        let mut classifier = VadClassifier::new(ScriptedModel {
            fixed_log_odds: 10.0, // Pre-temperature, p > 0.99
        });
        let f = dummy_features();

        for _ in 0..50 {
            classifier.process(&f, -60.0);
        }
        assert!(
            classifier.duck_gain > 0.9,
            "Gain charges near 1.0 during speech"
        );

        // Release run
        classifier.model.fixed_log_odds = -10.0; // Pre-temperature, p < 0.01

        // Wait for hysteresis hold to expire (15 frames)
        for _ in 0..16 {
            classifier.process(&f, -60.0);
        }

        let mut prev_gain = classifier.duck_gain;
        let mut frames_to_zero = 0;
        for _ in 0..100 {
            let d = classifier.process(&f, -60.0);
            assert!(
                d.duck_gain < prev_gain || d.duck_gain < 1e-4,
                "Gain decays monotonically"
            );
            prev_gain = d.duck_gain;
            if d.duck_gain > 0.05 {
                frames_to_zero += 1;
            }
        }

        assert!(
            frames_to_zero > 50,
            "Release is noticeably slower than attack"
        );
    }

    #[test]
    fn oracle_determinism() {
        let mut class_a = VadClassifier::new(FixedPriors);
        let mut class_b = VadClassifier::new(FixedPriors);

        let f = VadFeatures {
            spectral_flatness: 0.42,
            rms_db: -33.3,
            rms_delta_db: 2.1,
            mid_side_ratio: 0.11,
            transient_density: 0.0,
            mfcc: [0.0; 13],
        };

        let da = class_a.process(&f, -55.5);
        let db = class_b.process(&f, -55.5);

        assert_eq!(da.posterior.to_bits(), db.posterior.to_bits());
        assert_eq!(da.duck_gain.to_bits(), db.duck_gain.to_bits());
        assert_eq!(da.is_speech, db.is_speech);
    }

    #[test]
    fn oracle_nan_safety() {
        let mut classifier = VadClassifier::new(FixedPriors);

        // Inject poison
        let f_poison = VadFeatures {
            spectral_flatness: f32::NAN,
            rms_db: f32::INFINITY,
            rms_delta_db: 0.0,
            mid_side_ratio: f32::NAN,
            transient_density: 0.0,
            mfcc: [0.0; 13],
        };

        let d = classifier.process(&f_poison, f32::NAN);
        assert_eq!(d.posterior, 0.5); // Guard gives log_odds 0.0
        assert!(
            !d.duck_gain.is_nan(),
            "Poison must not infect duck_gain state"
        );

        // Assert the next 3 frames are clean, because the ring sanitized the Inf.
        for _ in 0..3 {
            let f_clean = dummy_features();
            let d_clean = classifier.process(&f_clean, -60.0);
            assert!(!d_clean.posterior.is_nan(), "Ring corruption survived!");
        }
    }

    struct ProbeModel;
    impl LikelihoodModel for ProbeModel {
        fn log_odds(&self, _f: &VadFeatures, ctx: &VadContext) -> f32 {
            ctx.rms_delta_30ms // Return delta verbatim
        }
    }

    #[test]
    fn oracle_ring_delta() {
        let mut classifier = VadClassifier::new(ProbeModel);

        let mut f = dummy_features();
        f.rms_db = -40.0;

        // Frame 0: ring has 1 element, not full -> neutral 0.0
        let d0 = classifier.process(&f, -60.0);
        let expected_p0 = 1.0 / (1.0 + libm::expf(0.0 / ProbeModel.temperature()));
        assert_eq!(d0.posterior, expected_p0);

        // Frame 1: ring has 2 elements
        f.rms_db = -30.0;
        let d1 = classifier.process(&f, -60.0);
        assert_eq!(d1.posterior, expected_p0);

        // Frame 2: ring has 3 elements
        f.rms_db = -20.0;
        let d2 = classifier.process(&f, -60.0);
        assert_eq!(d2.posterior, expected_p0);

        // Frame 3: ring is full. rms is -10. Oldest was -40. Delta = 30.
        f.rms_db = -10.0;
        let d3 = classifier.process(&f, -60.0);
        let expected_p3 = 1.0 / (1.0 + libm::expf(-30.0 / ProbeModel.temperature()));
        assert_eq!(d3.posterior, expected_p3);
    }
}
