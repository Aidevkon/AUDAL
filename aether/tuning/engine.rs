// aether/tuning/engine.rs — AteEngine
// Authority: spec/locked/S-008_autotuning_engine.md v1.0
// Single-pass — no re-render loop in v1.0.
// v2.0: iterative re-render (re-render → measure → update → repeat)
//       deferred until S-009 + full render pipeline are stable.

use lineos_types::analysis::StemFeatures;
use crate::personas::config::{PersonaOverride, PERSONA_OVERRIDE_DELTA_MAX};
use super::types::*;

pub struct AteEngine;

impl AteEngine {
    /// Single-pass deviation between reference and output features.
    /// Uses libm::log10f — no std::f32 methods (constitutional rule).
    pub fn compute_deviation(reference: &StemFeatures,
                              output:    &StemFeatures) -> DeviationVector {
        // Tilt: log ratio of mix centroids (empirical, documented)
        let ref_c  = reference.mix.spectral_centroid_hz.max(1.0_f32);
        let out_c  = output.mix.spectral_centroid_hz.max(1.0_f32);
        let d_tilt = 20.0_f32 * libm::log10f(ref_c / out_c);

        // Body: bass RMS difference (dB)
        let d_body = reference.bass.rms_db - output.bass.rms_db;

        // Transients: drums crest factor diff, normalized to [-1,1]
        let d_trans = ((reference.drums.crest_factor_db
                       - output.drums.crest_factor_db) / 20.0_f32)
                       .clamp(-1.0_f32, 1.0_f32);

        // Loudness: LUFS difference
        let d_loud = reference.mix.integrated_lufs
                   - output.mix.integrated_lufs;

        let dv = DeviationVector {
            delta_tilt_db:       d_tilt,
            delta_body_db:       d_body,
            delta_transients:    d_trans,
            delta_loudness_lufs: d_loud,
            converged:           false,
        };
        DeviationVector { converged: dv.is_converged(), ..dv }
    }

    /// Map deviation to PersonaOverride.
    /// All deltas clamped to PERSONA_OVERRIDE_DELTA_MAX.
    /// Smoothness not mapped (no reliable metric in v1.0).
    pub fn deviation_to_override(dev: &DeviationVector)
        -> PersonaOverride
    {
        // Warmth: tilt (brightness) + body (low-mid)
        let warmth_delta = (
            dev.delta_tilt_db * ATE_TILT_TO_WARMTH
          + dev.delta_body_db * ATE_BODY_TO_WARMTH
        ).clamp(-PERSONA_OVERRIDE_DELTA_MAX, PERSONA_OVERRIDE_DELTA_MAX);

        // Punch: transients
        let punch_delta = (
            dev.delta_transients * ATE_TRANS_TO_PUNCH
        ).clamp(-PERSONA_OVERRIDE_DELTA_MAX, PERSONA_OVERRIDE_DELTA_MAX);

        // Forwardness: loudness + tilt
        let forwardness_delta = (
            dev.delta_loudness_lufs * ATE_LOUD_TO_FORWARDNESS
          + dev.delta_tilt_db      * ATE_TILT_TO_FORWARDNESS
        ).clamp(-PERSONA_OVERRIDE_DELTA_MAX, PERSONA_OVERRIDE_DELTA_MAX);

        // Smoothness: not mapped in v1.0
        let smoothness_delta = 0.0_f32;

        PersonaOverride {
            warmth_delta,
            punch_delta,
            forwardness_delta,
            smoothness_delta,
        }
    }

    /// Single-pass tune: compute deviation → map to override.
    /// Accepts already-analyzed StemFeatures (not raw PCM).
    /// Feature extraction (PCM → StemFeatures) happens in LineOS (S-002)
    /// before crossing the Aether boundary — layer isolation compliant.
    pub fn tune(reference: &StemFeatures,
                output:    &StemFeatures) -> AteResult {
        let deviation        = Self::compute_deviation(reference, output);
        let persona_override = Self::deviation_to_override(&deviation);
        AteResult {
            converged: deviation.converged,
            deviation,
            persona_override,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::analysis::{StemFeatures, StemMetrics, MixMetrics};

    fn test_features() -> StemFeatures {
        StemFeatures {
            bass:      StemMetrics::default(),
            harmonics: StemMetrics::default(),
            drums:     StemMetrics::default(),
            ambience:  StemMetrics::default(),
            mix:       MixMetrics::default(),
        }
    }

    fn test_features_brighter() -> StemFeatures {
        let mut f = test_features();
        f.mix.spectral_centroid_hz = 3000.0;  // brighter
        f
    }

    fn test_features_darker() -> StemFeatures {
        let mut f = test_features();
        f.mix.spectral_centroid_hz = 500.0;   // darker
        f
    }

    #[test]
    fn ate_deviation_deterministic() {
        let r = test_features_brighter();
        let o = test_features_darker();
        assert_eq!(AteEngine::compute_deviation(&r, &o),
                   AteEngine::compute_deviation(&r, &o));
    }

    #[test]
    fn ate_identical_zero_deviation() {
        let f = test_features();
        let d = AteEngine::compute_deviation(&f, &f);
        assert!(d.delta_tilt_db.abs()       < 1e-4);
        assert!(d.delta_body_db.abs()       < 1e-4);
        assert!(d.delta_transients.abs()    < 1e-4);
        assert!(d.delta_loudness_lufs.abs() < 1e-4);
    }

    #[test]
    fn ate_identical_converged() {
        let f = test_features();
        assert!(AteEngine::compute_deviation(&f, &f).converged);
    }

    #[test]
    fn ate_override_bounded() {
        let dev = DeviationVector {
            delta_tilt_db:99.0, delta_body_db:-99.0,
            delta_transients:-99.0, delta_loudness_lufs:99.0,
            converged:false,
        };
        let ov = AteEngine::deviation_to_override(&dev);
        assert!(ov.warmth_delta.abs()
            <= PERSONA_OVERRIDE_DELTA_MAX + 1e-5);
        assert!(ov.punch_delta.abs()
            <= PERSONA_OVERRIDE_DELTA_MAX + 1e-5);
        assert!(ov.forwardness_delta.abs()
            <= PERSONA_OVERRIDE_DELTA_MAX + 1e-5);
        assert_eq!(ov.smoothness_delta, 0.0);
    }

    #[test]
    fn ate_tune_deterministic() {
        let r = test_features_brighter();
        let o = test_features_darker();
        let r1 = AteEngine::tune(&r, &o);
        let r2 = AteEngine::tune(&r, &o);
        assert_eq!(r1.persona_override.warmth_delta,
                   r2.persona_override.warmth_delta);
    }

    #[test]
    fn ate_tune_identical_converged() {
        let f = test_features();
        assert!(AteEngine::tune(&f, &f).converged);
    }

    #[test]
    fn ate_warmth_positive_when_output_dark() {
        // ref brighter than output → warmth_delta > 0
        let reference = test_features_brighter();
        let output    = test_features_darker();
        let dev = AteEngine::compute_deviation(&reference, &output);
        assert!(dev.delta_tilt_db > 0.0,
            "ref brighter → positive tilt");
        let ov = AteEngine::deviation_to_override(&dev);
        assert!(ov.warmth_delta > 0.0,
            "positive tilt → positive warmth_delta");
    }

    #[test]
    fn ate_smoothness_always_zero_v1() {
        let dev = DeviationVector {
            delta_tilt_db:1.0, delta_body_db:1.0,
            delta_transients:1.0, delta_loudness_lufs:1.0,
            converged:false,
        };
        assert_eq!(AteEngine::deviation_to_override(&dev)
                   .smoothness_delta, 0.0);
    }

    #[test]
    fn ate_result_serializable() {
        let f      = test_features();
        let result = AteEngine::tune(&f, &f);
        let _: AteResult = serde_json::from_str(
            &serde_json::to_string(&result).unwrap()).unwrap();
    }
}
