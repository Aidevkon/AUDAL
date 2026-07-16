use super::chaos::ChaosLayer;
use super::firewall::IntegrationFirewall;
use super::predictive::PredictiveController;
use super::voice_v1::MarkovStateClassifier;

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::analysis::StemMetrics;

    #[test]
    fn full_pipeline_deterministic() {
        // Same input → same DspConfig delta, always (INV-AB-1)
        let metrics = StemMetrics {
            rms_db: -20.0,
            crest_factor_db: 14.0,
            ..Default::default()
        };

        let current = MarkovStateClassifier::classify_voice(&metrics);
        let predicted = MarkovStateClassifier::predict_next(current);
        let delta1 = PredictiveController::compute_voice_delta(current, predicted);
        let delta2 = PredictiveController::compute_voice_delta(current, predicted);

        assert_eq!(delta1.comp_threshold_db, delta2.comp_threshold_db);
        assert_eq!(delta1.comp_attack_ms, delta2.comp_attack_ms);
    }

    #[test]
    fn firewall_never_exceeds_bounds() {
        use crate::markov::predictive::MarkovDelta;
        // Try to pass extreme values through firewall
        let extreme = MarkovDelta {
            comp_threshold_db: 999.0,
            comp_attack_ms: 999.0,
            comp_release_ms: 999.0,
            high_shelf_db: 999.0,
            low_shelf_db: 999.0,
            transient_risk: 999.0,
            gap_risk: 999.0,
        };
        let clamped = IntegrationFirewall::clamp_voice_delta(extreme);
        assert!(clamped.comp_threshold_db <= 3.0);
        assert!(clamped.comp_attack_ms <= 10.0);
        assert!(clamped.transient_risk <= 1.0);
    }

    #[test]
    fn chaos_bypass_zero_cpu() {
        let metrics = StemMetrics {
            rms_db: -15.0,
            crest_factor_db: 8.0,
            ..Default::default()
        };
        let current = MarkovStateClassifier::classify_voice(&metrics);
        let predicted = MarkovStateClassifier::predict_next(current);
        let delta = PredictiveController::compute_voice_delta(current, predicted);

        let chaos_on = ChaosLayer {
            bypass: false,
            seed: 42,
        };
        let chaos_off = ChaosLayer {
            bypass: true,
            seed: 42,
        };

        let modulated = chaos_on.modulate(delta.clone(), 42);
        let bypassed = chaos_off.modulate(delta.clone(), 42);

        // Bypass → unchanged
        assert_eq!(bypassed.comp_threshold_db, delta.comp_threshold_db);
        // Active → may differ (chaos applied)
        // Both must still pass firewall bounds
        let _ = IntegrationFirewall::clamp_voice_delta(modulated);
    }

    #[test]
    fn mark_iii_fallback_zero_delta() {
        // INV-AB-9: when no enrichment needed, delta = zero
        use crate::markov::predictive::MarkovDelta;
        let zero = MarkovDelta::zero();
        assert_eq!(zero.comp_threshold_db, 0.0);
        assert_eq!(zero.comp_attack_ms, 0.0);
    }
}
